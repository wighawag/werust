<!-- dorfl-sidecar: item=task:mobile-ipfs-scheme-interception-ios-and-android type=task slug=mobile-ipfs-scheme-interception-ios-and-android allAnswered=false -->

## Q1

**'task:mobile-ipfs-scheme-interception-ios-and-android' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Android: is resolveIpfs safe to call from shouldInterceptRequest, which Android runs on a WebView WORKER thread, while the UI thread independently drives the same CoreSession? The JNI nativeResolveIpfs rebuilds a &mut CoreSession via session(handle) and resolve_scheme does inner.borrow_mut() on an Rc<RefCell> (!Sync). A concurrent UI-thread core call (navigate/onPageStarted/onPageFinished, common during an in-flight load with sub-resource interception) yields two live &mut to the same object across threads plus a non-atomic RefCell borrow: a data race / UB / RefCell panic. There is no lock and no runOnUiThread marshalling. Desktop is sound only because WebKitGTK dispatches the handler on the single GTK thread (its install_ipfs comment says so); iOS WKURLSchemeHandler is main-thread too. Android alone breaks that single-thread assumption, so it needs a synchronization boundary (e.g. a Mutex around the session, or marshalling resolve onto a consistent thread). (BrowserActivity.CoreWebViewClient.shouldInterceptRequest -> WerustCore.resolveIpfs -> nativeResolveIpfs (session() = &mut) -> AndroidBackend::resolve_scheme (inner.borrow_mut); webview-renderer/src/backend.rs install_ipfs notes the Rc lifecycle is sound ONLY because the GTK loop is single-threaded. No Mutex/RwLock/synchronized/runOnUiThread anywhere in crates/werust-android.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:mobile-ipfs-scheme-interception-ios-and-android' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Android: is resolveIpfs safe to call from shouldInterceptRequest, which Android runs on a WebView WORKER thread, while the UI thread independently drives the same CoreSession? The JNI nativeResolveIpfs rebuilds a &mut CoreSession via session(handle) and resolve_scheme does inner.borrow_mut() on an Rc<RefCell> (!Sync). A concurrent UI-thread core call (navigate/onPageStarted/onPageFinished, common during an in-flight load with sub-resource interception) yields two live &mut to the same object across threads plus a non-atomic RefCell borrow: a data race / UB / RefCell panic. There is no lock and no runOnUiThread marshalling. Desktop is sound only because WebKitGTK dispatches the handler on the single GTK thread (its install_ipfs comment says so); iOS WKURLSchemeHandler is main-thread too. Android alone breaks that single-thread assumption, so it needs a synchronization boundary (e.g. a Mutex around the session, or marshalling resolve onto a consistent thread). (BrowserActivity.CoreWebViewClient.shouldInterceptRequest -> WerustCore.resolveIpfs -> nativeResolveIpfs (session() = &mut) -> AndroidBackend::resolve_scheme (inner.borrow_mut); webview-renderer/src/backend.rs install_ipfs notes the Rc lifecycle is sound ONLY because the GTK loop is single-threaded. No Mutex/RwLock/synchronized/runOnUiThread anywhere in crates/werust-android.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:mobile-ipfs-scheme-interception-ios-and-android' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Android: is resolveIpfs safe to call from shouldInterceptRequest, which Android runs on a WebView WORKER thread, while the UI thread independently drives the same CoreSession? The JNI nativeResolveIpfs rebuilds a &mut CoreSession via session(handle) and resolve_scheme does inner.borrow_mut() on an Rc<RefCell> (!Sync). A concurrent UI-thread core call (navigate/onPageStarted/onPageFinished, common during an in-flight load with sub-resource interception) yields two live &mut to the same object across threads plus a non-atomic RefCell borrow: a data race / UB / RefCell panic. There is no lock and no runOnUiThread marshalling. Desktop is sound only because WebKitGTK dispatches the handler on the single GTK thread (its install_ipfs comment says so); iOS WKURLSchemeHandler is main-thread too. Android alone breaks that single-thread assumption, so it needs a synchronization boundary (e.g. a Mutex around the session, or marshalling resolve onto a consistent thread). (BrowserActivity.CoreWebViewClient.shouldInterceptRequest -> WerustCore.resolveIpfs -> nativeResolveIpfs (session() = &mut) -> AndroidBackend::resolve_scheme (inner.borrow_mut); webview-renderer/src/backend.rs install_ipfs notes the Rc lifecycle is sound ONLY because the GTK loop is single-threaded. No Mutex/RwLock/synchronized/runOnUiThread anywhere in crates/werust-android.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):

## Q4

**'task:mobile-ipfs-scheme-interception-ios-and-android' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - Android: is resolveIpfs safe to call from shouldInterceptRequest, which Android runs on a WebView WORKER thread, while the UI thread independently drives the same CoreSession? The JNI nativeResolveIpfs rebuilds a &mut CoreSession via session(handle) and resolve_scheme does inner.borrow_mut() on an Rc<RefCell> (!Sync). A concurrent UI-thread core call (navigate/onPageStarted/onPageFinished, common during an in-flight load with sub-resource interception) yields two live &mut to the same object across threads plus a non-atomic RefCell borrow: a data race / UB / RefCell panic. There is no lock and no runOnUiThread marshalling. Desktop is sound only because WebKitGTK dispatches the handler on the single GTK thread (its install_ipfs comment says so); iOS WKURLSchemeHandler is main-thread too. Android alone breaks that single-thread assumption, so it needs a synchronization boundary (e.g. a Mutex around the session, or marshalling resolve onto a consistent thread). (BrowserActivity.CoreWebViewClient.shouldInterceptRequest -> WerustCore.resolveIpfs -> nativeResolveIpfs (session() = &mut) -> AndroidBackend::resolve_scheme (inner.borrow_mut); webview-renderer/src/backend.rs install_ipfs notes the Rc lifecycle is sound ONLY because the GTK loop is single-threaded. No Mutex/RwLock/synchronized/runOnUiThread anywhere in crates/werust-android.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q4 fields: id=q4 kind=stuck -->

**Your answer** (write below this line):
