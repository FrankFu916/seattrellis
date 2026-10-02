import Foundation
import Combine

public enum SolverStatus: String, CaseIterable, Sendable {
    case solved = "Solved", provenInfeasible = "ProvenInfeasible", timeout = "Timeout"
    case unknown = "Unknown", invalidInput = "InvalidInput", cancelled = "Cancelled", internalError = "InternalError"
    public var explanation: String {
        switch self {
        case .solved: return "A feasible plan passed independent validation."
        case .provenInfeasible: return "The solver proved that these hard constraints have no solution."
        case .timeout: return "The time budget ended. This does not prove infeasibility."
        case .unknown: return "No solution was found; infeasibility was not proved."
        case .invalidInput: return "The input needs correction before solving."
        case .cancelled: return "The operation was cancelled."
        case .internalError: return "An internal error prevented a verified result."
        }
    }
}

public struct EditorStudent: Codable, Identifiable, Equatable, Sendable {
    public let studentKey: String
    public let displayName: String
    public let seatID: String?
    public let locked: Bool
    public var id: String { studentKey }
    enum CodingKeys: String, CodingKey {
        case studentKey = "student_key", displayName = "display_name", seatID = "seat_id", locked
    }
}
public struct EditorSeat: Codable, Identifiable, Equatable, Sendable {
    public let seatID: String
    public let row: Int
    public let column: Int
    public let enabled: Bool
    public let studentKey: String?
    public let locked: Bool
    public var id: String { seatID }
    enum CodingKeys: String, CodingKey {
        case seatID = "seat_id", row, column = "col", enabled, studentKey = "student_key", locked
    }
}
public struct EditorState: Codable, Equatable, Sendable {
    public let draftID: String
    public let revision: UInt64
    public let undoDepth: UInt64
    public let redoDepth: UInt64
    public let students: [EditorStudent]
    public let seats: [EditorSeat]
    public let validation: JSONValue?
    enum CodingKeys: String, CodingKey {
        case draftID = "draft_id", revision, undoDepth = "undo_depth", redoDepth = "redo_depth"
        case students, seats, validation
    }
}

public struct RosterRow: Identifiable, Sendable {
    public let source: JSONValue
    public var id: String { source["id"].string ?? source["student_id"].string ?? source["key"].string ?? "" }
    public var name: String { source["name"].string ?? source["display_name"].string ?? id }
    public var height: String { (source["heightCm"] != .null ? source["heightCm"] : source["height_cm"]).display }
    public var vision: String { source["vision"].display }
    public var score: String { source["score"].display }
}

public struct SeatCoordinate: Hashable, Sendable {
    public let row: Int
    public let column: Int
    public init(row: Int, column: Int) { self.row = row; self.column = column }
}
public struct SeatGridProjection: Sendable {
    public let rows: [Int]
    public let columns: [Int]
    public let seats: [SeatCoordinate: EditorSeat]
    public let fallbackReason: String?
    public var canRenderGrid: Bool { fallbackReason == nil }
    public init(_ input: [EditorSeat]) {
        rows = Set(input.map(\.row)).sorted()
        columns = Set(input.map(\.column)).sorted()
        var lookup: [SeatCoordinate: EditorSeat] = [:]
        var duplicate = false
        for seat in input {
            if lookup.updateValue(seat, forKey: SeatCoordinate(row: seat.row, column: seat.column)) != nil { duplicate = true }
        }
        seats = lookup
        if duplicate { fallbackReason = "Overlapping custom coordinates are shown in the complete assignment list." }
        else if columns.isEmpty || rows.isEmpty { fallbackReason = "No grid cells are available." }
        else if rows.count > 2000 / columns.count { fallbackReason = "This large custom layout is shown in the complete assignment list." }
        else if input.count * 4 < rows.count * columns.count { fallbackReason = "This sparse custom layout is shown in the complete assignment list." }
        else { fallbackReason = nil }
    }
}

public enum BoundedFileAccess {
    public static let maximumDocumentBytes = 8 * 1024 * 1024 - 1024
    public static func read(_ url: URL) async throws -> Data {
        try await Task.detached(priority: .userInitiated) {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            let handle = try FileHandle(forReadingFrom: url)
            defer { try? handle.close() }
            var data = Data()
            while data.count <= maximumDocumentBytes {
                let block = try handle.read(upToCount: min(256 * 1024, maximumDocumentBytes + 1 - data.count)) ?? Data()
                if block.isEmpty { break }
                data.append(block)
            }
            guard data.count <= maximumDocumentBytes else { throw ClientFailure("The document exceeds the native protocol's 8 MiB limit.") }
            return data
        }.value
    }
    public static func write(_ data: Data, to url: URL) async throws {
        guard data.count <= NativeSession.maximumResponseBytes else { throw ClientFailure("The export exceeds the 32 MiB limit.") }
        try await Task.detached(priority: .userInitiated) {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            try data.write(to: url, options: .atomic)
        }.value
    }
}

public protocol DocumentFileAccess: Sendable {
    func read(_ url: URL) async throws -> Data
    func write(_ data: Data, to url: URL) async throws
}
public struct LocalDocumentFiles: DocumentFileAccess {
    public init() {}
    public func read(_ url: URL) async throws -> Data { try await BoundedFileAccess.read(url) }
    public func write(_ data: Data, to url: URL) async throws { try await BoundedFileAccess.write(data, to: url) }
}

@MainActor
public final class DocumentModel: ObservableObject {
    @Published public private(set) var source: JSONValue
    @Published public private(set) var generationRequest: JSONValue
    @Published public private(set) var editor: EditorState?
    @Published public private(set) var busy = false
    @Published public private(set) var busyOperation = ""
    @Published public private(set) var isDirty = false
    @Published public private(set) var fileURL: URL?
    @Published public private(set) var status: SolverStatus?
    @Published public private(set) var message = "Local preview · Import a JSON roster or use the fictional demonstration class."
    @Published public private(set) var auditReport: JSONValue?
    @Published public var errorMessage: String?
    @Published public var hasUnappliedInput = false
    private let transport: any NativeTransport
    private let files: any DocumentFileAccess
    private var sourceRevision: UInt64 = 0
    private var operationEpoch: UInt64 = 0
    private var cancelRequested = false

    public init(transport: any NativeTransport = NativeSession(), files: any DocumentFileAccess = LocalDocumentFiles()) {
        self.transport = transport
        self.files = files
        let request = Self.demonstrationRequest()
        generationRequest = request
        source = .object([
            "name": .string("Fictional demonstration class"),
            "students": request["draft"]["students"],
            "native_request": request,
            "client": .string("macos-native-preview"),
        ])
    }
    public var title: String { source["name"].string ?? "SeatTrellis class" }
    public var roster: [RosterRow] { (source["students"].array ?? []).map { RosterRow(source: $0) } }
    public var canCancel: Bool { busy && ["generate", "repair"].contains(busyOperation) }
    public var rosterJSON: String { (try? source["students"].text()) ?? "[]" }
    public var requestJSON: String { (try? generationRequest.text()) ?? "{}" }
    public var auditJSON: String {
        guard let auditReport else { return "No audit has been requested for the current revision." }
        return (try? auditReport.text()) ?? "The audit report could not be displayed."
    }

    public func cancel() {
        guard canCancel else { return }
        cancelRequested = true
        message = "Cancellation requested…"
        transport.cancel()
    }

    @discardableResult
    public func generate() async -> Bool {
        await perform("generate") {
            let epoch = self.operationEpoch, revision = self.sourceRevision
            let previous = self.editor
            let result = try await self.transport.dispatch(operation: "generate", payload: Self.adaptFrontendStudents(self.generationRequest))
            guard let rawStatus = result["status"].string, let status = SolverStatus(rawValue: rawStatus) else {
                throw ClientFailure("The solver returned an unrecognized status.")
            }
            if result["editor"] != .null {
                let next = try result["editor"].decode(EditorState.self)
                guard epoch == self.operationEpoch, revision == self.sourceRevision, !self.cancelRequested else {
                    _ = try await self.transport.dispatch(operation: "delete", payload: .object(["draft_id": .string(next.draftID)]))
                    self.status = .cancelled
                    self.message = SolverStatus.cancelled.explanation
                    return
                }
                self.editor = next
                self.isDirty = true
                self.auditReport = nil
                if let previous { await self.removePrevious(previous) }
            }
            self.status = status
            self.message = status.explanation
        }
    }

    /// Replacing input invalidates the old assignment. Unknown source fields
    /// remain in `source`, while each entire student record is retained.
    @discardableResult
    public func applyRosterJSON(_ text: String) async -> Bool {
        await perform("roster") {
            let students = try JSONValue.parse(Data(text.utf8))
            try await self.replaceRoster(students)
        }
    }

    /// This advanced editor exposes the complete request rather than a lossy
    /// subset of rules, layout, history and student attributes.
    @discardableResult
    public func applyRequestJSON(_ text: String) async -> Bool {
        await perform("request") {
            let request = try JSONValue.parse(Data(text.utf8))
            guard request.object != nil else { throw ClientFailure("The generation request must be a JSON object.") }
            let students = request["draft"].object == nil ? request["students"] : request["draft"]["students"]
            try Self.validateRoster(students)
            try Self.validateSingleCandidate(request)
            if let previous = self.editor { await self.removePrevious(previous) }
            self.source["students"] = students
            self.setInput(request)
            self.message = "Generation request updated. Generate to apply it."
        }
    }

    @discardableResult
    public func importRoster(from url: URL) async -> Bool {
        await perform("import") {
            let data = try await self.files.read(url)
            let value = try JSONValue.parse(data)
            let students = value.array == nil ? value["students"] : value
            try await self.replaceRoster(students)
        }
    }

    @discardableResult
    public func command(action: String = "apply", kind: String? = nil, payload: JSONValue = .object([:])) async -> Bool {
        await perform("command") {
            guard let current = self.editor else { throw ClientFailure("Generate or open a plan before editing.") }
            let operations: [JSONValue] = kind.map { [.object(["kind": .string($0), "payload": payload])] } ?? []
            let envelope = JSONValue.object([
                "kind": .string("seattrellis_editor_command"), "protocol_version": .string("1.0"),
                "command_id": .string(UUID().uuidString), "draft_id": .string(current.draftID),
                "base_revision": .unsigned(current.revision), "action": .string(action), "operations": .array(operations),
            ])
            let result = try await self.transport.dispatch(operation: "command", payload: envelope)
            let next = try result.decode(EditorState.self)
            guard next.draftID == current.draftID, next.revision > current.revision else {
                throw ClientFailure("An editor response did not advance the current revision.")
            }
            self.editor = next
            self.isDirty = true
            self.auditReport = nil
            self.message = "Edit applied · revision \(next.revision). Run Audit to inspect hard constraints."
        }
    }

    @discardableResult
    public func repair() async -> Bool {
        await perform("repair") {
            guard let current = self.editor else { throw ClientFailure("Open or generate a plan before repair.") }
            let result = try await self.transport.dispatch(operation: "repair", payload: .object([
                "draft_id": .string(current.draftID), "base_revision": .unsigned(current.revision),
                "affected_students": .array([]),
            ]))
            let next = try result.decode(EditorState.self)
            guard next.draftID == current.draftID, next.revision > current.revision else {
                throw ClientFailure("A repair response did not advance the current revision.")
            }
            self.editor = next
            self.isDirty = true
            self.auditReport = nil
            self.message = self.cancelRequested ? "Repair completed before cancellation could stop it. The committed revision is shown and can be undone."
                : "Repair applied and recorded as one undoable change."
        }
    }

    @discardableResult
    public func audit() async -> Bool {
        await perform("audit") {
            guard let current = self.editor else { throw ClientFailure("Open or generate a plan before auditing.") }
            let report = try await self.transport.dispatch(operation: "audit", payload: .object(["draft_id": .string(current.draftID)]))
            self.auditReport = report
            self.message = report["feasible"].bool == true ? "Audit: all hard constraints are satisfied." : "Audit: review the reported constraint violations."
        }
    }

    @discardableResult
    public func save(to url: URL) async -> Bool {
        await perform("save") {
            let references: [JSONValue] = self.editor.map { [.object(["draft_id": .string($0.draftID), "revision": .unsigned($0.revision)])] } ?? []
            let document = try await self.transport.dispatch(operation: "serialize", payload: .object([
                "class_source": self.source, "draft_refs": .array(references),
            ]))
            let data = try document.data(pretty: true)
            guard data.count <= BoundedFileAccess.maximumDocumentBytes else { throw ClientFailure("The class document exceeds the native protocol's reopening limit.") }
            try await self.files.write(data, to: url)
            self.fileURL = url
            self.isDirty = false
            self.message = "Saved \(url.lastPathComponent). The full input, assignment and locks are included."
        }
    }

    @discardableResult
    public func open(from url: URL) async -> Bool {
        await perform("open") {
            let document = try JSONValue.parse(try await self.files.read(url))
            guard let drafts = document["drafts"].array, drafts.count <= 1, document["rotation_plan"] == .null else {
                throw ClientFailure("This preview opens single-plan class documents. Candidate sets and rotation plans remain available in web and CLI.")
            }
            let incomingSource = document["class_source"]
            guard incomingSource.object != nil else { throw ClientFailure("The document has no complete class source.") }
            guard incomingSource["native_request"].object != nil || drafts.first?["solve_request"].object != nil else {
                throw ClientFailure("This unsolved class has no native generation request. Import its complete student JSON and configure a request, or generate and save it in the web product first.")
            }
            let request = incomingSource["native_request"].object != nil
                ? incomingSource["native_request"] : drafts[0]["solve_request"]
            try Self.validateSingleCandidate(request)
            let result = try await self.transport.dispatch(operation: "open", payload: document)
            let next = result["editor"] == .null ? nil : try result["editor"].decode(EditorState.self)
            let previous = self.editor
            self.source = result["class_source"]
            self.generationRequest = request
            self.editor = next
            self.sourceRevision += 1
            self.fileURL = url
            self.isDirty = false
            self.status = nil
            self.auditReport = nil
            self.message = "Opened \(url.lastPathComponent). Saved locks were restored; run Audit to inspect the plan."
            if let previous { await self.removePrevious(previous) }
        }
    }

    @discardableResult
    public func export(format: String, anonymize: Bool, to url: URL) async -> Bool {
        await perform("export") {
            guard let current = self.editor else { throw ClientFailure("Open or generate a plan before exporting.") }
            guard ["svg", "html"].contains(format) else { throw ClientFailure("This preview exports SVG and HTML.") }
            let result = try await self.transport.dispatch(operation: "export", payload: .object([
                "draft_id": .string(current.draftID), "format": .string(format),
                "options": .object([
                    "expected_revision": .unsigned(current.revision), "template": .string("public"),
                    "paper_size": .string("A4"), "orientation": .string("landscape"),
                    "privacy": .object(["anonymize": .bool(anonymize), "hide_scores": .bool(true),
                        "hide_notes": .bool(true), "hide_special_needs": .bool(true), "show_height": .bool(false), "show_vision": .bool(false)]),
                ]),
            ]))
            guard let encoded = result["base64"].string, let data = Data(base64Encoded: encoded) else {
                throw ClientFailure("The exporter returned an invalid artifact.")
            }
            try await self.files.write(data, to: url)
            let warnings = result["warnings"].array?.compactMap(\.string).joined(separator: " ") ?? ""
            self.message = "Exported \(url.lastPathComponent). \(warnings)"
        }
    }

    private func setInput(_ request: JSONValue) {
        generationRequest = request
        source["native_request"] = request
        sourceRevision += 1
        editor = nil
        status = nil
        auditReport = nil
        isDirty = true
    }
    private func replaceRoster(_ students: JSONValue) async throws {
        try Self.validateRoster(students)
        var request = generationRequest
        if request["draft"].object != nil {
            request["draft"]["students"] = .array(students.array!.map(Self.frontendStudent))
        } else {
            request["students"] = .array(try students.array!.map(Self.coreStudent))
            request["student_count"] = .integer(Int64(students.array!.count))
        }
        if let previous = editor { await removePrevious(previous) }
        source["students"] = students
        setInput(request)
        message = "Roster updated. Generate a new plan for this input."
    }
    private func removePrevious(_ previous: EditorState) async {
        do { _ = try await transport.dispatch(operation: "delete", payload: .object(["draft_id": .string(previous.draftID)])) }
        catch { message = "The previous native draft could not be released: \(error.localizedDescription)" }
    }
    private func perform(_ operation: String, action: () async throws -> Void) async -> Bool {
        guard !busy, !hasUnappliedInput || ["roster", "request"].contains(operation) else { return false }
        busy = true
        busyOperation = operation
        operationEpoch += 1
        cancelRequested = false
        errorMessage = nil
        defer { busy = false; busyOperation = "" }
        do { try await action(); return true }
        catch {
            if let failure = error as? ClientFailure {
                if failure.code == "cancelled" { status = .cancelled }
                else if failure.code == "repair_timeout" { status = .timeout }
                else if operation == "generate" { status = failure.status == 400 || failure.status == 422 ? .invalidInput : .internalError }
            } else if operation == "generate" { status = .internalError }
            present(error)
            return false
        }
    }
    public func present(_ error: Error) {
        errorMessage = error.localizedDescription
        message = error.localizedDescription
    }
    private static func validateRoster(_ value: JSONValue) throws {
        guard let students = value.array, !students.isEmpty, students.count <= 300 else {
            throw ClientFailure("Provide a JSON array containing between 1 and 300 student objects.")
        }
        var identifiers = Set<String>()
        for student in students {
            guard student.object != nil, let identifier = student["id"].string ?? student["student_id"].string ?? student["key"].string,
                  !identifier.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, identifier.count <= 128,
                  identifiers.insert(identifier).inserted else {
                throw ClientFailure("Each student needs a unique, nonempty id, student_id or core key of at most 128 characters.")
            }
            guard student["name"].string != nil || student["display_name"].string != nil else {
                throw ClientFailure("Each student needs a name (or core display_name).")
            }
        }
    }
    private static func validateSingleCandidate(_ request: JSONValue) throws {
        if request["options"]["candidate_count"] != .null && request["options"]["candidate_count"] != .integer(1) {
            throw ClientFailure("This preview supports one candidate per document. Use the web or CLI product for candidate sets.")
        }
    }
    private static func coreStudent(_ student: JSONValue) throws -> JSONValue {
        var core = student
        core["key"] = student["key"] != .null ? student["key"]
            : (student["id"] != .null ? student["id"] : student["student_id"])
        core["display_name"] = student["display_name"] != .null ? student["display_name"] : student["name"]
        if student["heightCm"] != .null { core["height_cm"] = student["heightCm"] }
        return core
    }
    private static func frontendStudent(_ student: JSONValue) -> JSONValue {
        var item = student
        item["student_id"] = student["id"] != .null ? student["id"]
            : (student["student_id"] != .null ? student["student_id"] : student["key"])
        if student["name"] == .null { item["name"] = student["display_name"] }
        if student["heightCm"] != .null { item["height_cm"] = student["heightCm"] }
        return item
    }
    private static func adaptFrontendStudents(_ request: JSONValue) -> JSONValue {
        guard request["draft"].object != nil, let students = request["draft"]["students"].array else { return request }
        var result = request
        result["draft"]["students"] = .array(students.map(frontendStudent))
        return result
    }
    public static func demonstrationRequest() -> JSONValue {
        var students: [JSONValue] = []
        for index in 1...12 {
            let identifier = "DEMO-\(index)"
            let height = Int64(140 + index)
            let score = Int64(60 + index)
            var student: [String: JSONValue] = [:]
            student["id"] = .string(identifier)
            student["student_id"] = .string(identifier)
            student["name"] = .string("Demo student \(index)")
            student["heightCm"] = .integer(height)
            student["height_cm"] = .integer(height)
            student["score"] = .integer(score)
            student["vision"] = .string("normal")
            student["tags"] = .array([])
            student["needs"] = .array([])
            student["attributes"] = .object(["fictional": .bool(true)])
            students.append(.object(student))
        }
        let room = JSONValue.object(["template_id": .string("standard-30")])
        let goal = JSONValue.object(["goal_id": .string("daily-rotation")])
        let draft = JSONValue.object([
            "students": .array(students), "room": room, "goal": goal,
            "history_snapshots": .array([]),
        ])
        let options = JSONValue.object([
            "seed": .integer(42), "time_limit_seconds": .number(2), "candidate_count": .integer(1),
        ])
        return .object(["draft": draft, "options": options])
    }
}
