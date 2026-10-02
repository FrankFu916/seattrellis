import SwiftUI
import AppKit
import SeatTrellisKit

@MainActor
struct WorkbenchView: View {
    @ObservedObject var document: DocumentModel
    let coordinator: ApplicationDelegate
    @State private var section: String? = "plan"
    @State private var selectedStudent = ""
    @State private var selectedSeat = ""
    @State private var swapStudent = ""
    @State private var listMode = false
    @State private var exportFormat = "svg"
    @State private var anonymize = false
    @State private var inputEditor: InputEditorKind?

    var body: some View {
        NavigationSplitView {
            List(selection: $section) {
                Label("Seating Plan", systemImage: "square.grid.3x3").tag("plan")
                Label("Students", systemImage: "person.3").tag("roster")
                Label("Audit", systemImage: "checkmark.shield").tag("audit")
                Section("Class") {
                    Text(document.title).font(.headline)
                    Text("\(document.roster.count) students")
                    Text(document.fileURL?.lastPathComponent ?? "Unsaved class").font(.caption).foregroundStyle(.secondary)
                }
                Section("Solver") {
                    Text(document.status?.rawValue ?? "Not generated")
                    if document.busy { ProgressView(document.busyOperation.capitalized) }
                    Text(document.message).font(.caption).textSelection(.enabled)
                }
                Section("Preview") {
                    Text("One class · one candidate\nAll computation stays on this Mac.").font(.caption).foregroundStyle(.secondary)
                }
            }
            .navigationSplitViewColumnWidth(min: 200, ideal: 230)
        } content: {
            VStack(spacing: 0) {
                if section == "roster" { rosterView }
                else if section == "audit" { auditView }
                else { planView }
                Divider()
                HStack {
                    Text(document.message).font(.caption).lineLimit(3).textSelection(.enabled)
                    Spacer()
                    if document.isDirty { Label("Unsaved", systemImage: "circle.fill").font(.caption) }
                }.padding(10)
            }
            .navigationSplitViewColumnWidth(min: 440, ideal: 620)
        } detail: {
            inspector
                .navigationSplitViewColumnWidth(min: 240, ideal: 280)
        }
        .toolbar {
            ToolbarItemGroup {
                Button { Task { await coordinator.open(document) } } label: { Label("Open", systemImage: "folder") }
                    .disabled(document.busy)
                Button { Task { await coordinator.save(document) } } label: { Label("Save", systemImage: "square.and.arrow.down") }
                    .disabled(document.busy)
                Divider()
                Button { Task { await document.generate() } } label: { Label("Generate", systemImage: "sparkles") }
                    .disabled(document.busy)
                if document.canCancel {
                    Button("Cancel", role: .cancel) { document.cancel() }.keyboardShortcut(.escape, modifiers: [])
                }
                Button { Task { await document.command(action: "undo") } } label: { Label("Undo", systemImage: "arrow.uturn.backward") }
                    .disabled(document.busy || (document.editor?.undoDepth ?? 0) == 0)
                Button { Task { await document.command(action: "redo") } } label: { Label("Redo", systemImage: "arrow.uturn.forward") }
                    .disabled(document.busy || (document.editor?.redoDepth ?? 0) == 0)
            }
        }
        .sheet(item: $inputEditor) { kind in
            JSONInputEditor(document: document, kind: kind)
        }
        .alert("Operation could not be completed", isPresented: Binding(
            get: { document.errorMessage != nil }, set: { if !$0 { document.errorMessage = nil } }
        )) {
            Button("OK") { document.errorMessage = nil }
        } message: { Text(document.errorMessage ?? "") }
    }

    private var rosterView: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Complete student source").font(.title2)
                Spacer()
                Button("Import JSON…") { Task { await coordinator.importRoster(document) } }
                Button("Edit JSON…") { openInput(.roster) }
            }.padding([.horizontal, .top])
            Text("The table is a projection. Saving retains every student field, including attributes, notes, needs, height, vision and score.")
                .font(.caption).foregroundStyle(.secondary).padding(.horizontal)
            Table(document.roster) {
                TableColumn("ID", value: \.id)
                TableColumn("Name", value: \.name)
                TableColumn("Height", value: \.height)
                TableColumn("Vision", value: \.vision)
                TableColumn("Score", value: \.score)
            }
            .accessibilityLabel("Complete student roster")
        }.disabled(document.busy)
    }
    private var auditView: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Current revision audit").font(.title2)
                Spacer()
                Button("Run Audit") { Task { await document.audit() } }.disabled(document.busy || document.editor == nil)
            }
            if let report = document.auditReport {
                Label(report["feasible"].bool == true ? "Hard constraints satisfied" : "Review violations",
                      systemImage: report["feasible"].bool == true ? "checkmark.shield" : "exclamationmark.triangle")
            }
            ScrollView { Text(document.auditJSON).font(.system(.body, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) }
        }.padding()
    }
    private var planView: some View {
        VStack(alignment: .leading) {
            HStack {
                Text("Seating plan").font(.title2)
                Spacer()
                Toggle("Accessible list", isOn: $listMode).toggleStyle(.switch)
            }.padding()
            if let editor = document.editor {
                let projection = SeatGridProjection(editor.seats)
                Text("Front of classroom · revision \(editor.revision)").font(.caption).foregroundStyle(.secondary).padding(.horizontal)
                if let reason = projection.fallbackReason { Text(reason).font(.caption).foregroundStyle(.secondary).padding(.horizontal) }
                if listMode || !projection.canRenderGrid { assignmentTable(editor) }
                else { seatGrid(editor, projection: projection) }
            } else {
                Spacer()
                VStack(spacing: 12) {
                    Image(systemName: "square.grid.3x3").font(.system(size: 44)).foregroundStyle(.secondary)
                    Text("Generate your first plan").font(.title2)
                    Text("Use the fictional demonstration roster, import student JSON, or open a saved class.")
                        .multilineTextAlignment(.center).foregroundStyle(.secondary)
                    Button("Generate") { Task { await document.generate() } }.disabled(document.busy)
                }.frame(maxWidth: .infinity).padding()
                Spacer()
            }
        }
    }
    private func assignmentTable(_ editor: EditorState) -> some View {
        Table(editor.students) {
            TableColumn("Student", value: \.displayName)
            TableColumn("ID", value: \.studentKey)
            TableColumn("Seat") { student in Text(student.seatID ?? "Unseated") }
            TableColumn("Lock") { student in Text(student.locked ? "Locked" : "Unlocked") }
        }.accessibilityLabel("Seat assignments, including unseated students")
    }
    private func seatGrid(_ editor: EditorState, projection: SeatGridProjection) -> some View {
        return ScrollView([.horizontal, .vertical]) {
            Grid(alignment: .topLeading, horizontalSpacing: 8, verticalSpacing: 8) {
                ForEach(projection.rows, id: \.self) { row in
                    GridRow {
                        ForEach(projection.columns, id: \.self) { column in
                            if let seat = projection.seats[SeatCoordinate(row: row, column: column)] {
                                seatButton(seat, editor: editor)
                            } else { Text("Aisle").font(.caption).foregroundStyle(.secondary).frame(width: 108, height: 72) }
                        }
                    }
                }
            }.padding()
        }.accessibilityLabel("Classroom seating chart. Use the accessible list or inspector for keyboard editing.")
    }
    private func seatButton(_ seat: EditorSeat, editor: EditorState) -> some View {
        let student = editor.students.first { $0.studentKey == seat.studentKey }
        return Button {
            selectedSeat = seat.seatID
            if let student { selectedStudent = student.studentKey }
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(seat.seatID).font(.caption).foregroundStyle(.secondary)
                    Spacer()
                    if seat.locked || student?.locked == true { Image(systemName: "lock.fill").font(.caption) }
                }
                Text(student?.displayName ?? "Empty").lineLimit(2).frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(9).frame(width: 108, height: 72)
            .background(selectedSeat == seat.seatID ? Color.accentColor.opacity(0.2) : Color.primary.opacity(0.06))
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(selectedSeat == seat.seatID ? Color.accentColor : .clear, lineWidth: 2))
        }.buttonStyle(.plain).disabled(document.busy || !seat.enabled)
            .accessibilityLabel("\(seat.seatID), row \(seat.row), column \(seat.column), \(student?.displayName ?? "empty"), \(seat.locked ? "seat locked" : "seat unlocked"), \(student?.locked == true ? "student locked" : "student unlocked")")
    }

    private var inspector: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Plan controls").font(.headline)
                Button("Generation Request…") { openInput(.request) }.disabled(document.busy)
                Text("The complete request includes seed, time budget, room, rules and history.").font(.caption).foregroundStyle(.secondary)
                Divider()
                if let editor = document.editor {
                    Picker("Student", selection: $selectedStudent) {
                        Text("Choose student").tag("")
                        ForEach(editor.students) { Text("\($0.displayName) · \($0.studentKey)").tag($0.studentKey) }
                    }
                    Picker("Destination", selection: $selectedSeat) {
                        Text("Choose seat").tag("")
                        ForEach(editor.seats.filter(\.enabled)) { Text($0.seatID).tag($0.seatID) }
                    }
                    Button("Move to Seat") { edit("move_student", ["student_key": .string(selectedStudent), "seat_id": .string(selectedSeat)]) }
                        .disabled(selectedStudent.isEmpty || selectedSeat.isEmpty)
                    Button("Unseat Student") { edit("unseat_student", ["student_key": .string(selectedStudent)]) }.disabled(selectedStudent.isEmpty)
                    Picker("Swap with", selection: $swapStudent) {
                        Text("Choose student").tag("")
                        ForEach(editor.students) { Text($0.displayName).tag($0.studentKey) }
                    }
                    Button("Swap Students") { edit("swap_students", ["first_student": .string(selectedStudent), "second_student": .string(swapStudent)]) }
                        .disabled(selectedStudent.isEmpty || swapStudent.isEmpty || selectedStudent == swapStudent)
                    Divider()
                    let studentLocked = editor.students.first { $0.studentKey == selectedStudent }?.locked == true
                    Button(studentLocked ? "Unlock Student" : "Lock Student") {
                        edit(studentLocked ? "unlock_student" : "lock_student", ["student_key": .string(selectedStudent)])
                    }.disabled(selectedStudent.isEmpty)
                    let seatLocked = editor.seats.first { $0.seatID == selectedSeat }?.locked == true
                    Button(seatLocked ? "Unlock Seat" : "Lock Seat") {
                        edit(seatLocked ? "unlock_seat" : "lock_seat", ["seat_id": .string(selectedSeat)])
                    }.disabled(selectedSeat.isEmpty)
                    Divider()
                    Button("Repair Plan") { Task { await document.repair() } }
                    Button("Audit Plan") { section = "audit"; Task { await document.audit() } }
                    Text("Manual edits can violate hard rules. Audit explains violations; repair preserves locks.")
                        .font(.caption).foregroundStyle(.secondary)
                    Divider()
                    Picker("Export", selection: $exportFormat) {
                        Text("SVG").tag("svg")
                        Text("HTML").tag("html")
                    }
                    Toggle("Anonymize names", isOn: $anonymize)
                    Text("Public export hides scores, notes, needs, height and vision.").font(.caption).foregroundStyle(.secondary)
                    Button("Export…") { Task { await coordinator.export(document, format: exportFormat, anonymize: anonymize) } }
                } else {
                    Text("Generate or open a plan to adjust seats, locks, repair, audit and export.").foregroundStyle(.secondary)
                }
            }.padding().disabled(document.busy)
        }
    }
    private func edit(_ kind: String, _ payload: [String: JSONValue]) {
        Task { await document.command(kind: kind, payload: .object(payload)) }
    }
    private func openInput(_ kind: InputEditorKind) {
        Task { if await coordinator.confirmReplacement(document) { inputEditor = kind } }
    }
}

enum InputEditorKind: String, Identifiable {
    case roster, request
    var id: String { rawValue }
}
@MainActor
struct JSONInputEditor: View {
    @ObservedObject var document: DocumentModel
    let kind: InputEditorKind
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var original = ""
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(kind == .roster ? "Edit complete student JSON" : "Edit generation request").font(.title2)
            Text("Applying input replaces the current editable plan. Unknown source fields and complete student records are retained.")
                .font(.caption).foregroundStyle(.secondary)
            TextEditor(text: $text).font(.system(.body, design: .monospaced))
                .accessibilityLabel(kind == .roster ? "Complete student JSON" : "Complete generation request JSON")
            if let error = document.errorMessage { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { cancelEditing() }.disabled(document.busy)
                Button("Apply Input") {
                    Task {
                        let applied = kind == .roster ? await document.applyRosterJSON(text) : await document.applyRequestJSON(text)
                        if applied { dismiss() }
                    }
                }.keyboardShortcut(.defaultAction).disabled(document.busy)
            }
        }.padding(20).frame(minWidth: 720, minHeight: 520)
            .interactiveDismissDisabled(document.busy || text != original)
            .onAppear { original = kind == .roster ? document.rosterJSON : document.requestJSON; text = original }
            .onChange(of: text) { document.hasUnappliedInput = $0 != original }
            .onDisappear { document.hasUnappliedInput = false }
    }
    private func cancelEditing() {
        if text != original {
            let alert = NSAlert()
            alert.messageText = "Discard unapplied JSON edits?"
            alert.informativeText = "The saved class and current plan have not been changed."
            alert.addButton(withTitle: "Discard Edits")
            alert.addButton(withTitle: "Keep Editing")
            guard alert.runModal() == .alertFirstButtonReturn else { return }
        }
        dismiss()
    }
}
