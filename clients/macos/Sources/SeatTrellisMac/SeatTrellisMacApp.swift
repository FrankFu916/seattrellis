import SwiftUI
import AppKit
import UniformTypeIdentifiers
import SeatTrellisKit

@main
@MainActor
struct SeatTrellisMacApp: App {
    @NSApplicationDelegateAdaptor(ApplicationDelegate.self) private var delegate
    @StateObject private var document = DocumentModel()

    var body: some Scene {
        // Window, rather than WindowGroup, deliberately owns one native session.
        Window("SeatTrellis · Native Preview", id: "class-document") {
            WorkbenchView(document: document, coordinator: delegate)
                .frame(minWidth: 1000, minHeight: 650)
                .background(WindowConnection(coordinator: delegate, document: document))
        }
        .defaultSize(width: 1200, height: 800)
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Open Class…") { Task { await delegate.open(document) } }
                    .keyboardShortcut("o").disabled(document.busy)
                Button("Import Student JSON…") { Task { await delegate.importRoster(document) } }
                    .keyboardShortcut("i").disabled(document.busy)
            }
            CommandGroup(replacing: .saveItem) {
                Button("Save Class") { Task { await delegate.save(document) } }
                    .keyboardShortcut("s").disabled(document.busy)
                Button("Save Class As…") { Task { await delegate.save(document, forcePanel: true) } }
                    .keyboardShortcut("s", modifiers: [.command, .shift]).disabled(document.busy)
            }
            CommandGroup(replacing: .undoRedo) {
                Button("Undo Plan Edit") { Task { await document.command(action: "undo") } }
                    .keyboardShortcut("z").disabled(document.busy || (document.editor?.undoDepth ?? 0) == 0)
                Button("Redo Plan Edit") { Task { await document.command(action: "redo") } }
                    .keyboardShortcut("z", modifiers: [.command, .shift]).disabled(document.busy || (document.editor?.redoDepth ?? 0) == 0)
            }
            CommandMenu("Plan") {
                Button("Generate") { Task { await document.generate() } }
                    .keyboardShortcut(.return, modifiers: .command).disabled(document.busy)
                Button("Cancel Operation") { document.cancel() }.disabled(!document.canCancel)
                Divider()
                Button("Repair Plan") { Task { await document.repair() } }.disabled(document.busy || document.editor == nil)
                Button("Audit Plan") { Task { await document.audit() } }.disabled(document.busy || document.editor == nil)
            }
        }
    }
}

@MainActor
final class ApplicationDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    weak var window: NSWindow?
    var document: DocumentModel?
    private var approvedClose = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard !approvedClose, let document else { return true }
        guard !document.hasUnappliedInput else { showUnappliedInput(); return false }
        guard !document.busy else { showBusyClose(); return false }
        guard document.isDirty else { approvedClose = true; return true }
        Task {
            if await confirmReplacement(document) {
                approvedClose = true
                sender.performClose(nil)
            }
        }
        return false
    }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard !approvedClose, let document else { return .terminateNow }
        guard !document.hasUnappliedInput else { showUnappliedInput(); return .terminateCancel }
        guard !document.busy else { showBusyClose(); return .terminateCancel }
        guard document.isDirty else { return .terminateNow }
        Task {
            let approved = await confirmReplacement(document)
            approvedClose = approved
            sender.reply(toApplicationShouldTerminate: approved)
        }
        return .terminateLater
    }
    private func showBusyClose() {
        let alert = NSAlert()
        alert.messageText = "An operation is still running"
        alert.informativeText = "Wait for it to finish, or cancel a generation or repair before closing this class."
        alert.addButton(withTitle: "Keep Open")
        alert.runModal()
    }
    private func showUnappliedInput() {
        let alert = NSAlert()
        alert.messageText = "Finish editing the JSON input"
        alert.informativeText = "Apply the input, or use Cancel and confirm discarding the unapplied edits, before replacing or closing this class."
        alert.addButton(withTitle: "Keep Editing")
        alert.runModal()
    }
    func confirmReplacement(_ document: DocumentModel) async -> Bool {
        guard !document.hasUnappliedInput else { showUnappliedInput(); return false }
        guard !document.busy else { return false }
        guard document.isDirty else { return true }
        let alert = NSAlert()
        alert.messageText = "Save changes to this class?"
        alert.informativeText = "Your complete student input, current assignment and locks can be saved as a portable class document."
        alert.addButton(withTitle: "Save…")
        alert.addButton(withTitle: "Discard Changes")
        alert.addButton(withTitle: "Cancel")
        switch alert.runModal() {
        case .alertFirstButtonReturn: return await save(document)
        case .alertSecondButtonReturn: return true
        default: return false
        }
    }
    @discardableResult
    func save(_ document: DocumentModel, forcePanel: Bool = false) async -> Bool {
        guard !document.busy else { return false }
        if !forcePanel, let url = document.fileURL { return await document.save(to: url) }
        let panel = NSSavePanel()
        panel.title = "Save Portable Class"
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = document.fileURL?.lastPathComponent ?? "class.seattrellis.json"
        guard panel.runModal() == .OK, let url = panel.url else { return false }
        return await document.save(to: url)
    }
    func open(_ document: DocumentModel) async {
        guard await confirmReplacement(document) else { return }
        let panel = NSOpenPanel()
        panel.title = "Open Portable Class"
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        await document.open(from: url)
    }
    func importRoster(_ document: DocumentModel) async {
        guard await confirmReplacement(document) else { return }
        let panel = NSOpenPanel()
        panel.title = "Import Student JSON"
        panel.message = "Choose an array of complete student objects, or an object containing a students array."
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        await document.importRoster(from: url)
    }
    func export(_ document: DocumentModel, format: String, anonymize: Bool) async {
        guard !document.busy, document.editor != nil else { return }
        let panel = NSSavePanel()
        panel.title = "Export Seating Plan"
        panel.allowedContentTypes = [format == "html" ? .html : (UTType(filenameExtension: "svg") ?? .data)]
        panel.nameFieldStringValue = "seating-plan.\(format)"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        await document.export(format: format, anonymize: anonymize, to: url)
    }
}

@MainActor
struct WindowConnection: NSViewRepresentable {
    let coordinator: ApplicationDelegate
    let document: DocumentModel
    func makeNSView(context: Context) -> NSView { NSView() }
    func updateNSView(_ view: NSView, context: Context) {
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            coordinator.window = window
            coordinator.document = document
            window.delegate = coordinator
            window.isDocumentEdited = document.isDirty
            window.representedURL = document.fileURL
        }
    }
}
