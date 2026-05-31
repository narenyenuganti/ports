import PortsBarCore

// Thin entry point: the App scene and all logic live in the PortsBarCore
// library so they can be unit-tested. swift-testing cannot be hosted by a test
// target that links an executable's `@main`, so the executable is a minimal
// shim that delegates to the library.
PortsBarApp.main()
