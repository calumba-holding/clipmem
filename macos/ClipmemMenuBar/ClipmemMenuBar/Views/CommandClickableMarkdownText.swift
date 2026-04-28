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
            guard event.type == .leftMouseDown, event.modifierFlags.contains(.command) else {
                return event
            }

            let point = view.convert(event.locationInWindow, from: nil)
            guard view.bounds.contains(point), let link = link(at: point, in: view.bounds.size) else {
                return event
            }

            let target = LinkTargetResolver.classify(link.target)
            guard target != .unsupported else { return event }
            onOpen?(target)
            return nil
        }

        private func link(at point: NSPoint, in size: NSSize) -> MarkdownRenderedLink? {
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
            let pointInText = NSPoint(x: point.x - usedRect.minX, y: point.y - usedRect.minY)
            guard pointInText.x >= 0, pointInText.y >= 0 else { return nil }

            let glyphIndex = layoutManager.glyphIndex(for: pointInText, in: textContainer)
            let characterIndex = layoutManager.characterIndexForGlyph(at: glyphIndex)
            guard characterIndex < attributedString.string.utf16.count else { return nil }
            return links.first { NSLocationInRange(characterIndex, $0.range) }
        }
    }
}

private final class LinkCommandClickMonitorView: NSView {
    weak var coordinator: LinkCommandClickMonitor.Coordinator?
    private var monitor: Any?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        installMonitor()
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        guard newWindow == nil, let monitor else { return }
        NSEvent.removeMonitor(monitor)
        self.monitor = nil
    }

    private func installMonitor() {
        if let monitor {
            NSEvent.removeMonitor(monitor)
            self.monitor = nil
        }

        guard window != nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard let self else { return event }
            return coordinator?.handle(event, in: self) ?? event
        }
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
