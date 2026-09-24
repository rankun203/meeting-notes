import SwiftUI

// Some Command Line Tools SDKs expose a State macro whose implementation ships
// only in Xcode. This alias selects the original native SwiftUI property wrapper
// so the same app builds with either developer toolchain.
typealias ViewState<Value> = SwiftUI.State<Value>
