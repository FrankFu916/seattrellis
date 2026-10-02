import Foundation
import CSeattrellis

public struct ClientFailure: LocalizedError, Sendable {
    public let code: String
    public let message: String
    public let status: Int?
    public init(_ message: String, code: String = "client_error", status: Int? = nil) {
        self.message = message; self.code = code; self.status = status
    }
    public var errorDescription: String? { message }
}

public protocol NativeTransport: Sendable {
    func dispatch(operation: String, payload: JSONValue) async throws -> JSONValue
    func cancel()
}

/// One serial session per document. Cancellation bypasses the dispatch queue,
/// and buffers are copied and freed before any result reaches the UI.
public final class NativeSession: NativeTransport, @unchecked Sendable {
    private let handle: UInt64
    private let queue = DispatchQueue(label: "org.seattrellis.native.document", qos: .userInitiated)
    private let lock = NSLock()
    private var cancellationRequested = false
    private var busy = false
    private var operationToken = UUID()
    private var cancellationTimer: DispatchSourceTimer?
    private let cancellationQueue = DispatchQueue(label: "org.seattrellis.native.cancel", qos: .userInitiated)
    private let beforeDispatch: (@Sendable () -> Void)?
    private let initializationError: ClientFailure?
    public static let maximumInputBytes = 8 * 1024 * 1024
    public static let maximumResponseBytes = 32 * 1024 * 1024

    public convenience init() { self.init(beforeDispatch: nil) }
    // Internal deterministic handshake seam for the cancellation regression.
    init(beforeDispatch: (@Sendable () -> Void)?) {
        self.beforeDispatch = beforeDispatch
        if seattrellis_abi_version() != 1 {
            handle = 0
            initializationError = ClientFailure("The native library uses an unsupported ABI version.")
        } else {
            handle = seattrellis_session_create()
            initializationError = handle == 0 ? ClientFailure("Could not allocate a native document session.") : nil
        }
    }
    deinit { if handle != 0 { seattrellis_session_destroy(handle) } }

    public func dispatch(operation: String, payload: JSONValue) async throws -> JSONValue {
        if let initializationError { throw initializationError }
        let request = JSONValue.object([
            "protocol_version": .integer(1), "operation": .string(operation), "payload": payload,
        ])
        let bytes = try request.data()
        guard bytes.count <= Self.maximumInputBytes else { throw ClientFailure("The request exceeds the 8 MiB limit.") }
        try beginCall()
        return try await withCheckedThrowingContinuation { continuation in
            queue.async { [self] in
                let outcome: Result<JSONValue, Error>
                do {
                    if isCancellationRequested() { throw ClientFailure("Operation cancelled.", code: "cancelled") }
                    beforeDispatch?()
                    let buffer = bytes.withUnsafeBytes { storage in
                        seattrellis_session_dispatch(handle, storage.bindMemory(to: UInt8.self).baseAddress, bytes.count)
                    }
                    defer { seattrellis_buffer_free(buffer) }
                    guard let pointer = buffer.data, buffer.len > 0, buffer.len <= Self.maximumResponseBytes else {
                        throw ClientFailure("The native library could not return a bounded response.", code: "resource_limit")
                    }
                    let response = try JSONValue.parse(Data(bytes: pointer, count: buffer.len))
                    guard response["protocol_version"] == .integer(1), let okay = response["ok"].bool else {
                        throw ClientFailure("The native library returned an invalid protocol response.")
                    }
                    guard okay else {
                        let failure = response["error"]
                        let status = try? failure["status"].decode(Int.self)
                        throw ClientFailure(failure["message"].string ?? "Native operation failed.",
                            code: failure["code"].string ?? "native_error", status: status)
                    }
                    outcome = .success(response["result"])
                } catch { outcome = .failure(error) }
                finishCall()
                continuation.resume(with: outcome)
            }
        }
    }
    public func cancel() {
        lock.lock()
        defer { lock.unlock() }
        guard busy else { return }
        cancellationRequested = true
        if seattrellis_session_cancel(handle) == 0, cancellationTimer == nil {
            // The Rust call may not have published its control yet. Retry only
            // for this call's token; finishCall cancels the timer under the same
            // lock before a fresh call can begin.
            let token = operationToken
            let timer = DispatchSource.makeTimerSource(queue: cancellationQueue)
            timer.schedule(deadline: .now(), repeating: .milliseconds(1), leeway: .milliseconds(0))
            timer.setEventHandler { [weak self] in self?.retryCancellation(token: token) }
            cancellationTimer = timer
            timer.resume()
        }
    }
    private func beginCall() throws {
        lock.lock(); defer { lock.unlock() }
        guard !busy else { throw ClientFailure("The native session is busy.", code: "busy", status: 409) }
        busy = true
        cancellationRequested = false
        operationToken = UUID()
    }
    private func finishCall() {
        lock.lock()
        cancellationTimer?.cancel(); cancellationTimer = nil
        busy = false
        lock.unlock()
    }
    private func retryCancellation(token: UUID) {
        lock.lock(); defer { lock.unlock() }
        guard busy, cancellationRequested, operationToken == token else { return }
        if seattrellis_session_cancel(handle) != 0 {
            cancellationTimer?.cancel(); cancellationTimer = nil
        }
    }
    private func isCancellationRequested() -> Bool { lock.lock(); defer { lock.unlock() }; return cancellationRequested }
}
