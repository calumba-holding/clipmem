import AppKit
import SwiftUI

struct CommandClickableMarkdownText: View {
    let rendered: MarkdownRenderedText
    var lineLimit: Int?
    var truncationMode: Text.TruncationMode = .tail
    var selectionEnabled = false

    @ViewBuilder
    var body: some View {
        let text = Text(rendered.attributed)
            .lineLimit(lineLimit)
            .truncationMode(truncationMode)
            .linkCommandClickMonitor(
                attributedString: rendered.attributed,
                links: rendered.links,
                lineLimit: lineLimit,
                truncationMode: truncationMode
            )

        if selectionEnabled {
            text.textSelection(.enabled)
        } else {
            text.textSelection(.disabled)
        }
    }
}

private extension View {
    func linkCommandClickMonitor(
        attributedString: AttributedString,
        links: [MarkdownRenderedLink],
        lineLimit: Int?,
        truncationMode: Text.TruncationMode
    ) -> some View {
        background {
            LinkCommandClickMonitor(
                attributedString: attributedString,
                links: links,
                lineLimit: lineLimit,
                truncationMode: truncationMode
            ) { target in
                PasteboardActions.openLinkTarget(target)
            }
            .allowsHitTesting(false)
        }
    }
}

private struct LinkCommandClickMonitor: NSViewRepresentable {
    let attributedString: AttributedString
    let links: [MarkdownRenderedLink]
    let lineLimit: Int?
    let truncationMode: Text.TruncationMode
    let onOpen: (LinkTargetResolution) -> Void

    func makeNSView(context: Context) -> LinkCommandClickMonitorView {
        let view = LinkCommandClickMonitorView()
        view.coordinator = context.coordinator
        return view
    }

    func updateNSView(_ nsView: LinkCommandClickMonitorView, context: Context) {
        context.coordinator.configure(
            attributedString: attributedString,
            links: links,
            lineLimit: lineLimit,
            lineBreakMode: truncationMode.nsLineBreakMode,
            onOpen: onOpen
        )
        nsView.coordinator = context.coordinator
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    @MainActor
    final class Coordinator {
        private var attributedString = NSAttributedString()
        private var links: [MarkdownRenderedLink] = []
        private var lineLimit: Int?
        private var lineBreakMode: NSLineBreakMode = .byTruncatingTail
        private var onOpen: ((LinkTargetResolution) -> Void)?

        func configure(
            attributedString: AttributedString,
            links: [MarkdownRenderedLink],
            lineLimit: Int?,
            lineBreakMode: NSLineBreakMode,
            onOpen: @escaping (LinkTargetResolution) -> Void
        ) {
            self.attributedString = NSAttributedString(attributedString)
            self.links = links
            self.lineLimit = lineLimit
            self.lineBreakMode = lineBreakMode
            self.onOpen = onOpen
        }

        func handle(_ event: NSEvent, in view: NSView) -> NSEvent? {
            if event.type != .leftMouseDown {
                return event
            }
            guard event.window === view.window else { return event }
            let point = view.convert(event.locationInWindow, from: nil)

            guard
                event.modifierFlags.contains(.command),
                let target = actionableTarget(at: point, in: view.bounds)
            else {
                return event
            }

            onOpen?(target)
            return nil
        }

        func isActionableCommandHover(_ event: NSEvent, in view: NSView) -> Bool {
            guard event.modifierFlags.contains(.command) else { return false }
            return actionableTarget(at: point(for: event, in: view), in: view.bounds) != nil
        }

        private func actionableTarget(at point: NSPoint, in bounds: NSRect) -> LinkTargetResolution? {
            guard bounds.contains(point) else { return nil }
            return MarkdownLinkHitTesting.actionableTarget(
                at: point,
                in: bounds.size,
                attributedString: attributedString,
                links: links,
                lineLimit: lineLimit,
                lineBreakMode: lineBreakMode
            )
        }

        private func point(for event: NSEvent, in view: NSView) -> NSPoint {
            if event.window === view.window {
                return view.convert(event.locationInWindow, from: nil)
            }
            guard let window = view.window else { return .zero }
            return view.convert(window.mouseLocationOutsideOfEventStream, from: nil)
        }
    }
}

private final class LinkCommandClickMonitorView: NSView {
    weak var coordinator: LinkCommandClickMonitor.Coordinator?
    private var monitor: Any?
    private var trackingArea: NSTrackingArea?
    private var isShowingPointingHand = false
    private weak var mouseMovedEventsWindow: NSWindow?
    private var previousAcceptsMouseMovedEvents: Bool?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        updateTrackingArea()
        installMonitor()
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        updateTrackingArea()
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        guard newWindow == nil else { return }
        removeMonitor()
        restoreCursorIfNeeded()
        restoreMouseMovedEventsPreference()
    }

    override func mouseMoved(with event: NSEvent) {
        updateCursor(for: event)
        super.mouseMoved(with: event)
    }

    override func mouseExited(with event: NSEvent) {
        restoreCursorIfNeeded()
        super.mouseExited(with: event)
    }

    private func installMonitor() {
        removeMonitor()
        guard window != nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .mouseMoved, .flagsChanged]) { [weak self] event in
            guard let self else { return event }
            updateCursor(for: event)
            return coordinator?.handle(event, in: self) ?? event
        }
    }

    private func removeMonitor() {
        if let monitor {
            NSEvent.removeMonitor(monitor)
            self.monitor = nil
        }
    }

    private func updateTrackingArea() {
        if let trackingArea {
            removeTrackingArea(trackingArea)
            self.trackingArea = nil
        }
        guard window != nil else { return }

        let area = NSTrackingArea(
            rect: .zero,
            options: [.activeInKeyWindow, .inVisibleRect, .mouseMoved, .mouseEnteredAndExited],
            owner: self
        )
        addTrackingArea(area)
        trackingArea = area
        guard let window else { return }
        if mouseMovedEventsWindow !== window {
            restoreMouseMovedEventsPreference()
            mouseMovedEventsWindow = window
            previousAcceptsMouseMovedEvents = window.acceptsMouseMovedEvents
        }
        window.acceptsMouseMovedEvents = true
    }

    private func updateCursor(for event: NSEvent) {
        guard coordinator?.isActionableCommandHover(event, in: self) == true else {
            restoreCursorIfNeeded()
            return
        }

        guard isShowingPointingHand == false else { return }
        NSCursor.pointingHand.set()
        isShowingPointingHand = true
    }

    private func restoreCursorIfNeeded() {
        guard isShowingPointingHand else { return }
        NSCursor.arrow.set()
        isShowingPointingHand = false
    }

    private func restoreMouseMovedEventsPreference() {
        guard let window = mouseMovedEventsWindow, let previousAcceptsMouseMovedEvents else {
            return
        }
        window.acceptsMouseMovedEvents = previousAcceptsMouseMovedEvents
        mouseMovedEventsWindow = nil
        self.previousAcceptsMouseMovedEvents = nil
    }
}

enum MarkdownLinkHitTesting {
    static func actionableTarget(
        at point: NSPoint,
        in size: NSSize,
        attributedString: NSAttributedString,
        links: [MarkdownRenderedLink],
        lineLimit: Int?,
        lineBreakMode: NSLineBreakMode
    ) -> LinkTargetResolution? {
        guard let link = link(
            at: point,
            in: size,
            attributedString: attributedString,
            links: links,
            lineLimit: lineLimit,
            lineBreakMode: lineBreakMode
        ) else {
            return nil
        }

        let target = LinkTargetResolver.classify(link.target)
        return target == .unsupported ? nil : target
    }

    static func link(
        at point: NSPoint,
        in size: NSSize,
        attributedString: NSAttributedString,
        links: [MarkdownRenderedLink],
        lineLimit: Int?,
        lineBreakMode: NSLineBreakMode
    ) -> MarkdownRenderedLink? {
        guard size.width > 0, size.height > 0 else { return nil }

        let textStorage = NSTextStorage(attributedString: attributedString)
        let layoutManager = NSLayoutManager()
        let textContainer = NSTextContainer(size: NSSize(width: size.width, height: CGFloat.greatestFiniteMagnitude))

        textContainer.lineFragmentPadding = 0
        textContainer.maximumNumberOfLines = lineLimit ?? 0
        textContainer.lineBreakMode = lineBreakMode
        textStorage.addLayoutManager(layoutManager)
        layoutManager.addTextContainer(textContainer)
        layoutManager.ensureLayout(for: textContainer)

        let usedRect = layoutManager.usedRect(for: textContainer)
        guard usedRect.contains(point) else { return nil }

        let pointInText = NSPoint(x: point.x - usedRect.minX, y: point.y - usedRect.minY)
        guard pointInText.x >= 0, pointInText.y >= 0 else { return nil }

        let glyphIndex = layoutManager.glyphIndex(for: pointInText, in: textContainer)
        let glyphRect = layoutManager
            .boundingRect(forGlyphRange: NSRange(location: glyphIndex, length: 1), in: textContainer)
            .offsetBy(dx: -usedRect.minX, dy: -usedRect.minY)
        guard glyphRect.contains(pointInText) else { return nil }

        let characterIndex = layoutManager.characterIndexForGlyph(at: glyphIndex)
        guard characterIndex < attributedString.string.utf16.count else { return nil }
        return links.first { NSLocationInRange(characterIndex, $0.range) }
    }
}

private extension Text.TruncationMode {
    var nsLineBreakMode: NSLineBreakMode {
        switch self {
        case .head:
            .byTruncatingHead
        case .middle:
            .byTruncatingMiddle
        case .tail:
            .byTruncatingTail
        @unknown default:
            .byTruncatingTail
        }
    }
}
