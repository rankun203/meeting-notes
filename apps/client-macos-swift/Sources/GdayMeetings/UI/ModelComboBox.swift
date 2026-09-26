import AppKit
import SwiftUI

/// Editable model field with the provider's models in its menu. NSComboBox is the
/// native macOS control for "type a value or pick one": it keeps free text entry
/// for endpoints without a usable list, and its menu supports arrow keys, Return,
/// and Escape. Typing filters the menu by ID or name, which matters for lists
/// with hundreds of models.
struct ModelComboBox: NSViewRepresentable {
    @Binding var text: String
    var models: [ProviderModel]

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSComboBox {
        let box = NSComboBox()
        box.usesDataSource = true
        box.dataSource = context.coordinator
        box.delegate = context.coordinator
        box.completes = false
        box.numberOfVisibleItems = 12
        box.placeholderString = "Model"
        box.stringValue = text
        box.setAccessibilityLabel("Model")
        box.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return box
    }

    func updateNSView(_ box: NSComboBox, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self
        // Leave the field alone while the person is typing in it.
        if box.stringValue != text, box.currentEditor() == nil { box.stringValue = text }
        if coordinator.models != models {
            coordinator.models = models
            coordinator.refilter(box.stringValue)
            box.reloadData()
        }
    }

    final class Coordinator: NSObject, NSComboBoxDataSource, NSComboBoxDelegate {
        var parent: ModelComboBox
        var models: [ProviderModel] = []
        private var visible: [ProviderModel] = []

        init(_ parent: ModelComboBox) { self.parent = parent }

        func refilter(_ query: String) { visible = ProviderModelList.filter(models, query: query) }

        func numberOfItems(in comboBox: NSComboBox) -> Int { visible.count }

        func comboBox(_ comboBox: NSComboBox, objectValueForItemAt index: Int) -> Any? {
            visible.indices.contains(index) ? visible[index].id : nil
        }

        func comboBox(_ comboBox: NSComboBox, indexOfItemWithStringValue string: String) -> Int {
            visible.firstIndex { $0.id == string } ?? NSNotFound
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            parent.text = box.stringValue
            refilter(box.stringValue)
            box.reloadData()
        }

        func comboBoxWillPopUp(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            refilter(box.stringValue)
            box.reloadData()
        }

        func comboBoxSelectionDidChange(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox,
                visible.indices.contains(box.indexOfSelectedItem)
            else { return }
            parent.text = visible[box.indexOfSelectedItem].id
        }
    }
}
