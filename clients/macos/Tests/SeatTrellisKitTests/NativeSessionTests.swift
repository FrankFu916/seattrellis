import Foundation
import XCTest
@testable import SeatTrellisKit

final class NativeSessionTests: XCTestCase {
    func testCancellationBeforeRustPublishesControlAndFreshCallIsolation() async throws {
        let entered = XCTestExpectation(description: "Swift call passed its cancellation check")
        let gate = OneShotDispatchGate(started: { entered.fulfill() })
        let session = NativeSession(beforeDispatch: { gate.enter() })
        var request = await DocumentModel.demonstrationRequest()
        var students: [JSONValue] = []
        for index in 1...40 {
            let identifier = "S\(index)"
            let height = Int64(140 + index)
            let student = JSONValue.object([
                "id": .string(identifier), "student_id": .string(identifier),
                "name": .string("Student \(index)"), "height_cm": .integer(height),
            ])
            students.append(student)
        }
        request["draft"]["students"] = .array(students)
        request["draft"]["room"]["template_id"] = .string("standard-60")
        let generating = Task { try await session.dispatch(operation: "generate", payload: request) }
        await fulfillment(of: [entered], timeout: 10)
        defer { gate.release.signal() }
        // The first call has passed the Swift cancellation check, but Rust is
        // deliberately still idle. This reproduces the publication window.
        do {
            _ = try await session.dispatch(operation: "state", payload: .object(["draft_id": .string("none")]))
            XCTFail("Overlapping calls must be rejected rather than queued")
        } catch let error as ClientFailure { XCTAssertEqual(error.code, "busy") }
        session.cancel()
        let cancellationTime = Date()
        gate.release.signal()
        do {
            let cancelled = try await generating.value
            XCTAssertEqual(cancelled["status"].string, "Cancelled")
        } catch let error as ClientFailure { XCTAssertEqual(error.code, "cancelled") }
        XCTAssertLessThan(Date().timeIntervalSince(cancellationTime), 5)
        let fresh = try await session.dispatch(operation: "generate", payload: await DocumentModel.demonstrationRequest())
        XCTAssertEqual(fresh["status"].string, "Solved")
        XCTAssertNotEqual(fresh["editor"], .null)
    }

    func testMalformedNativeRequestReturnsStructuredErrorAndSessionRemainsUsable() async throws {
        let session = NativeSession()
        do {
            _ = try await session.dispatch(operation: "unsupported-operation", payload: .object([:]))
            XCTFail("Unsupported operations must fail")
        } catch let error as ClientFailure {
            XCTAssertNotNil(error.status)
            XCTAssertFalse(error.message.isEmpty)
        }
        let result = try await session.dispatch(operation: "serialize", payload: .object([
            "class_source": .object(["opaque": .string("preserve")]), "draft_refs": .array([]),
        ]))
        XCTAssertEqual(result["class_source"]["opaque"], .string("preserve"))
    }
}

private final class OneShotDispatchGate: @unchecked Sendable {
    let started: @Sendable () -> Void
    let release = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var first = true
    init(started: @escaping @Sendable () -> Void) { self.started = started }
    func enter() {
        lock.lock()
        let shouldPause = first
        first = false
        lock.unlock()
        if shouldPause {
            started()
            _ = release.wait(timeout: .now() + 10)
        }
    }
}
