import Foundation
import XCTest
@testable import SeatTrellisKit

final class DocumentModelTests: XCTestCase {
    @MainActor
    func testNativeRoundTripEditingLocksRepairAuditAndExport() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let model = DocumentModel()
        let students = try JSONValue.parse(Data("""
        [
          {"id":"A","name":"Alice","heightCm":151,"vision":"normal","score":88,"notes":"private A","attributes":{"custom":{"retained":true}}},
          {"id":"B","name":"Bob","heightCm":163,"vision":"normal","score":74,"needs":["private need"]},
          {"id":"C","name":"Charlie","heightCm":158,"vision":"normal","score":81}
        ]
        """.utf8))
        assertSuccessful(await model.applyRosterJSON(try students.text()))
        assertSuccessful(await model.generate(), model.errorMessage ?? "Generation failed")
        XCTAssertEqual(model.status, .solved)
        let original = try XCTUnwrap(model.editor)
        let aSeat = try XCTUnwrap(original.students.first { $0.id == "A" }?.seatID)
        let bSeat = try XCTUnwrap(original.students.first { $0.id == "B" }?.seatID)

        assertSuccessful(await model.command(kind: "swap_students", payload: .object([
            "first_student": .string("A"), "second_student": .string("B"),
        ])))
        XCTAssertEqual(model.editor?.students.first { $0.id == "A" }?.seatID, bSeat)
        assertSuccessful(await model.command(action: "undo"))
        XCTAssertEqual(model.editor?.students.first { $0.id == "A" }?.seatID, aSeat)
        assertSuccessful(await model.command(action: "redo"))
        XCTAssertEqual(model.editor?.students.first { $0.id == "A" }?.seatID, bSeat)
        assertSuccessful(await model.command(kind: "lock_student", payload: .object(["student_key": .string("A")])))
        assertSuccessful(await model.command(kind: "lock_seat", payload: .object(["seat_id": .string(bSeat)])))

        let documentURL = directory.appendingPathComponent("class.json")
        assertSuccessful(await model.save(to: documentURL))
        XCTAssertFalse(model.isDirty)
        var saved = try JSONValue.parse(Data(contentsOf: documentURL))
        XCTAssertEqual(saved["class_source"]["students"], students)
        let preservedHeight = saved["drafts"].array?.first?["solve_request"]["students"].array?.first?["height_cm"]
        XCTAssertEqual(try preservedHeight?.decode(Double.self), 151)
        // Metadata from another client remains preserved through future saves.
        saved["class_source"]["other_client_metadata"] = .object(["opaque": .array([.integer(1), .string("preserve")])])
        try saved.data().write(to: documentURL)
        let reopened = DocumentModel()
        assertSuccessful(await reopened.open(from: documentURL))
        XCTAssertFalse(reopened.isDirty)
        XCTAssertEqual(reopened.source["students"], students)
        XCTAssertEqual(reopened.editor?.students.first { $0.id == "A" }?.seatID, bSeat)
        XCTAssertEqual(reopened.editor?.students.first { $0.id == "A" }?.locked, true)
        XCTAssertEqual(reopened.editor?.seats.first { $0.id == bSeat }?.locked, true)

        assertSuccessful(await reopened.command(kind: "unseat_student", payload: .object(["student_key": .string("C")])))
        XCTAssertNil(reopened.editor?.students.first { $0.id == "C" }?.seatID)
        assertSuccessful(await reopened.repair(), reopened.errorMessage ?? "Repair failed")
        XCTAssertTrue(reopened.editor?.students.allSatisfy { $0.seatID != nil } == true)
        XCTAssertEqual(reopened.editor?.students.first { $0.id == "A" }?.seatID, bSeat)
        assertSuccessful(await reopened.audit())
        XCTAssertEqual(reopened.auditReport?["feasible"], .bool(true))
        let exportURL = directory.appendingPathComponent("plan.svg")
        assertSuccessful(await reopened.export(format: "svg", anonymize: true, to: exportURL))
        let svg = try String(contentsOf: exportURL, encoding: .utf8)
        XCTAssertTrue(svg.contains("<svg"))
        XCTAssertFalse(svg.contains("private A"))
        XCTAssertFalse(svg.contains("private need"))
        XCTAssertFalse(svg.contains("Alice"))
        assertSuccessful(await reopened.save(to: documentURL))
        let resaved = try JSONValue.parse(Data(contentsOf: documentURL))
        XCTAssertEqual(resaved["class_source"]["other_client_metadata"], saved["class_source"]["other_client_metadata"])
    }

    @MainActor
    func testCancelledLateResultCannotReplaceDocumentAndFreshGenerationWorks() async throws {
        let transport = ControlledTransport()
        let model = DocumentModel(transport: transport)
        let generation = Task { await model.generate() }
        await transport.waitUntilStarted()
        XCTAssertTrue(model.busy)
        assertRejected(await model.applyRosterJSON("[{\"id\":\"X\",\"name\":\"New source\"}]"))
        model.cancel()
        await transport.completeFirst()
        _ = await generation.value
        XCTAssertNil(model.editor)
        XCTAssertEqual(model.status, .cancelled)
        assertSuccessful(await transport.deletedDraft)
        XCTAssertFalse(model.busy)
        assertSuccessful(await model.generate())
        XCTAssertEqual(model.status, .solved)
        XCTAssertEqual(model.editor?.draftID, "controlled-draft")
    }

    @MainActor
    func testImportOwnsDocumentWhileFileReadIsSuspended() async throws {
        let files = SuspendedFiles(data: Data("[{\"id\":\"IMPORTED\",\"name\":\"Complete roster\",\"attributes\":{\"opaque\":123}}]".utf8))
        let model = DocumentModel(transport: ControlledTransport(pauseFirst: false), files: files)
        let importing = Task { await model.importRoster(from: URL(fileURLWithPath: "/incoming.json")) }
        await files.waitUntilRead()
        XCTAssertTrue(model.busy)
        assertRejected(await model.open(from: URL(fileURLWithPath: "/other-class.json")))
        assertRejected(await model.generate())
        await files.completeRead()
        assertSuccessful(await importing.value)
        XCTAssertEqual(model.roster.map(\.id), ["IMPORTED"])
        XCTAssertEqual(model.source["students"].array?.first?["attributes"]["opaque"], .integer(123))
    }

    @MainActor
    func testUnsolvedForeignClassWithoutRequestIsRejectedWithoutChangingSource() async throws {
        let files = MemoryFiles(data: Data("""
        {"kind":"seattrellis_class_document","schema_version":1,"class_source":{"students":[{"id":"REAL","name":"Real student"}],"name":"Real class"},"drafts":[]}
        """.utf8))
        let model = DocumentModel(transport: ControlledTransport(pauseFirst: false), files: files)
        let original = model.source
        assertRejected(await model.open(from: URL(fileURLWithPath: "/foreign.json")))
        XCTAssertEqual(model.source, original)
        XCTAssertTrue(model.errorMessage?.contains("no native generation request") == true)
        XCTAssertNil(model.fileURL)
    }

    @MainActor
    func testSaveFailureLeavesDirtyDocumentAndCurrentPlanIntact() async throws {
        let model = DocumentModel(transport: ControlledTransport(pauseFirst: false), files: FailingWrites())
        assertSuccessful(await model.generate())
        let editor = model.editor
        assertRejected(await model.save(to: URL(fileURLWithPath: "/class.json")))
        XCTAssertTrue(model.isDirty)
        XCTAssertEqual(model.editor, editor)
        XCTAssertNil(model.fileURL)
    }

    func testJSONPreservesUnknownFieldsAndMaximumUnsignedSeed() throws {
        let raw = Data("{\"seed\":18446744073709551615,\"student\":{\"height\":151.5,\"vision\":\"low\",\"attributes\":{\"future\":[true,null,\"中文\"]}}}".utf8)
        let value = try JSONValue.parse(raw)
        XCTAssertEqual(value["seed"], .unsigned(UInt64.max))
        XCTAssertEqual(try JSONValue.parse(value.data()), value)
        XCTAssertEqual(Set(SolverStatus.allCases.map(\.rawValue)), Set(["Solved", "ProvenInfeasible", "Timeout", "Unknown", "InvalidInput", "Cancelled", "InternalError"]))
        XCTAssertNotEqual(SolverStatus.provenInfeasible.explanation, SolverStatus.unknown.explanation)
        XCTAssertNotEqual(SolverStatus.provenInfeasible.explanation, SolverStatus.timeout.explanation)
    }

    func testGridFallsBackForLargeSparseAndOverlappingCoordinates() {
        func seat(_ index: Int, row: Int, column: Int) -> EditorSeat {
            EditorSeat(seatID: "S\(index)", row: row, column: column, enabled: true, studentKey: nil, locked: false)
        }
        let regular = SeatGridProjection((0..<80).map { seat($0, row: $0 / 10 + 1, column: $0 % 10 + 1) })
        XCTAssertTrue(regular.canRenderGrid)
        XCTAssertEqual(regular.seats.count, 80)
        let large = SeatGridProjection((0..<10_000).map { seat($0, row: $0 + 1, column: $0 + 1) })
        XCTAssertFalse(large.canRenderGrid)
        XCTAssertEqual(large.seats.count, 10_000)
        let sparse = SeatGridProjection((0..<20).map { seat($0, row: $0 + 1, column: $0 + 1) })
        XCTAssertFalse(sparse.canRenderGrid)
        let overlapping = SeatGridProjection([seat(1, row: 1, column: 1), seat(2, row: 1, column: 1)])
        XCTAssertFalse(overlapping.canRenderGrid)
        XCTAssertTrue(overlapping.fallbackReason?.contains("Overlapping") == true)
    }
}

private let controlledEditor: JSONValue = try! JSONValue.parse(Data("""
{"kind":"seattrellis_editor_state","protocol_version":"1.0","draft_id":"controlled-draft","revision":0,"undo_depth":0,"redo_depth":0,"students":[{"student_key":"A","display_name":"A","seat_id":"R1C1","locked":false}],"seats":[{"seat_id":"R1C1","row":1,"col":1,"enabled":true,"student_key":"A","locked":false}]}
""".utf8))

private actor ControlledTransport: NativeTransport {
    private var pauseFirst: Bool
    private var started = false
    private var startWaiters: [CheckedContinuation<Void, Never>] = []
    private var completion: CheckedContinuation<Void, Never>?
    private(set) var deletedDraft = false
    init(pauseFirst: Bool = true) { self.pauseFirst = pauseFirst }
    nonisolated func cancel() {}
    func dispatch(operation: String, payload: JSONValue) async throws -> JSONValue {
        switch operation {
        case "generate":
            if pauseFirst {
                pauseFirst = false
                started = true
                startWaiters.forEach { $0.resume() }; startWaiters.removeAll()
                await withCheckedContinuation { completion = $0 }
            }
            return .object(["status": .string("Solved"), "feasible": .bool(true), "editor": controlledEditor])
        case "delete": deletedDraft = true; return .object(["deleted": .bool(true)])
        case "serialize": return .object(["kind": .string("seattrellis_class_document"), "schema_version": .integer(1), "class_source": payload["class_source"], "drafts": .array([])])
        default: throw ClientFailure("Unexpected test operation: \(operation)")
        }
    }
    func waitUntilStarted() async {
        if !started { await withCheckedContinuation { startWaiters.append($0) } }
    }
    func completeFirst() { completion?.resume(); completion = nil }
}
private actor SuspendedFiles: DocumentFileAccess {
    let data: Data
    private var started = false
    private var waiters: [CheckedContinuation<Void, Never>] = []
    private var completion: CheckedContinuation<Void, Never>?
    init(data: Data) { self.data = data }
    func read(_ url: URL) async throws -> Data {
        started = true
        waiters.forEach { $0.resume() }; waiters.removeAll()
        await withCheckedContinuation { completion = $0 }
        return data
    }
    func write(_ data: Data, to url: URL) async throws {}
    func waitUntilRead() async {
        if !started { await withCheckedContinuation { waiters.append($0) } }
    }
    func completeRead() { completion?.resume(); completion = nil }
}
private struct MemoryFiles: DocumentFileAccess {
    let data: Data
    func read(_ url: URL) async throws -> Data { data }
    func write(_ data: Data, to url: URL) async throws {}
}
private struct FailingWrites: DocumentFileAccess {
    func read(_ url: URL) async throws -> Data { throw ClientFailure("Unexpected read") }
    func write(_ data: Data, to url: URL) async throws { throw ClientFailure("Simulated disk failure") }
}

private func assertSuccessful(_ outcome: Bool, _ message: String = "", file: StaticString = #filePath, line: UInt = #line) {
    XCTAssertTrue(outcome, message, file: file, line: line)
}
private func assertRejected(_ outcome: Bool, _ message: String = "", file: StaticString = #filePath, line: UInt = #line) {
    XCTAssertFalse(outcome, message, file: file, line: line)
}
