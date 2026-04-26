import AppKit
import SwiftUI

struct ScrollElasticityDisabler: NSViewRepresentable {
    func makeNSView(context: Context) -> ScrollElasticityDisablerView {
        ScrollElasticityDisablerView()
    }

    func updateNSView(_ nsView: ScrollElasticityDisablerView, context: Context) {
        nsView.disableElasticity()
    }
}

final class ScrollElasticityDisablerView: NSView {
    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        disableElasticity()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        disableElasticity()
    }

    func disableElasticity() {
        DispatchQueue.main.async { [weak self] in
            guard let scrollView = Self.nearestScrollView(from: self) else { return }
            scrollView.verticalScrollElasticity = .none
            scrollView.horizontalScrollElasticity = .none
        }
    }

    private static func nearestScrollView(from view: NSView?) -> NSScrollView? {
        var currentView = view
        while let candidate = currentView {
            if let scrollView = candidate as? NSScrollView {
                return scrollView
            }
            if let scrollView = candidate.enclosingScrollView {
                return scrollView
            }
            currentView = candidate.superview
        }
        return nil
    }
}

extension View {
    func disablesScrollElasticity() -> some View {
        background {
            ScrollElasticityDisabler()
                .frame(width: 0, height: 0)
        }
    }
}
