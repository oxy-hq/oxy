//! A `Layer` wrapper that remembers the subscriber's `Dispatch`, for code that
//! needs a cross-layer lookup from inside an event callback.
//!
//! `tracing::dispatcher::get_default` cannot serve there: while a dispatcher is
//! dispatching, tracing's recursion guard hands any re-entrant caller the
//! no-op dispatcher, so a `tracing_opentelemetry::get_otel_context` through it
//! finds no OTel layer and silently answers `None`. The `Dispatch` handed to
//! [`Layer::on_register_dispatch`] is the real one; this wrapper keeps a weak
//! reference to it and delegates everything else to the layer it wraps.
//!
//! That hook is not enough on its own: `tracing-subscriber` 0.3.23 forwards it
//! through `Layered`, `Filtered`, `Box<dyn Layer>` and `Option<L>`, but **not**
//! through `Vec<L>` — and the binary composes its layers as a `Vec`. So the
//! caller also binds explicitly with [`DispatchHandle::bind`] once the
//! subscriber is installed, from outside any callback, where
//! `tracing::dispatcher::get_default` still answers with the real dispatcher.

use std::sync::{Arc, OnceLock};

use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::Interest;
use tracing::{Dispatch, Event, Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// A shared, late-bound handle to the subscriber's `Dispatch`. Cloning shares
/// the slot; [`DispatchHandle::get`] answers `None` until the subscriber is
/// installed, and again once it has been dropped.
#[derive(Clone, Debug, Default)]
pub struct DispatchHandle(Arc<OnceLock<tracing::dispatcher::WeakDispatch>>);

impl DispatchHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Option<Dispatch> {
        self.0.get()?.upgrade()
    }

    /// Bind explicitly. A no-op once bound. Call it right after the subscriber
    /// is installed — `tracing::dispatcher::get_default(|d| handle.bind(d))`
    /// — from code that is not itself inside a `tracing` callback.
    pub fn bind(&self, dispatch: &Dispatch) {
        let _ = self.0.set(dispatch.downgrade());
    }
}

/// The wrapper. Build it with [`WithDispatch::new`] and hand the same
/// [`DispatchHandle`] to whatever needs the lookup.
pub struct WithDispatch<L> {
    inner: L,
    handle: DispatchHandle,
}

impl<L> WithDispatch<L> {
    pub fn new(inner: L, handle: DispatchHandle) -> Self {
        Self { inner, handle }
    }
}

impl<L, S> Layer<S> for WithDispatch<L>
where
    L: Layer<S>,
    S: Subscriber,
{
    fn on_register_dispatch(&self, subscriber: &Dispatch) {
        self.handle.bind(subscriber);
        self.inner.on_register_dispatch(subscriber);
    }
    fn on_layer(&mut self, subscriber: &mut S) {
        self.inner.on_layer(subscriber);
    }
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        self.inner.max_level_hint()
    }
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        self.inner.register_callsite(metadata)
    }
    fn enabled(&self, metadata: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        self.inner.enabled(metadata, ctx)
    }
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        self.inner.on_new_span(attrs, id, ctx);
    }
    fn on_record(&self, span: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        self.inner.on_record(span, values, ctx);
    }
    fn on_follows_from(&self, span: &Id, follows: &Id, ctx: Context<'_, S>) {
        self.inner.on_follows_from(span, follows, ctx);
    }
    fn event_enabled(&self, event: &Event<'_>, ctx: Context<'_, S>) -> bool {
        self.inner.event_enabled(event, ctx)
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        self.inner.on_event(event, ctx);
    }
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        self.inner.on_enter(id, ctx);
    }
    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        self.inner.on_exit(id, ctx);
    }
    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        self.inner.on_close(id, ctx);
    }
    fn on_id_change(&self, old: &Id, new: &Id, ctx: Context<'_, S>) {
        self.inner.on_id_change(old, new, ctx);
    }
    unsafe fn downcast_raw(&self, id: std::any::TypeId) -> Option<*const ()> {
        // SAFETY: forwarded unchanged; the contract is the inner layer's.
        unsafe { self.inner.downcast_raw(id) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn the_handle_is_bound_when_the_subscriber_is_installed() {
        let handle = DispatchHandle::new();
        assert!(handle.get().is_none(), "nothing installed yet");
        let layer = WithDispatch::new(
            tracing_subscriber::fmt::layer().with_writer(std::io::sink),
            handle.clone(),
        );
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            assert!(handle.get().is_some(), "bound at registration");
        });
    }
}
