use super::{
    Anchor, AnchorHandle, AnchorInner, DirtyHandle, Engine, OutputContext, Poll, UpdateContext,
};
use std::cell::RefCell;
use std::rc::Rc;

/// An Anchor type for values that are mutated by calling a setter function from outside of the Anchors recomputation graph.
pub struct Var<T, H> {
    inner: Rc<RefCell<VarInner<T, H>>>,
    my_val: T,
}

#[derive(Clone)]
struct VarInner<T, H> {
    dirty_handle: Option<H>,
    val: Option<T>,
}

/// A setter that can update values inside an associated `Var`.
pub struct VarSetter<T, H> {
    inner: Rc<RefCell<VarInner<T, H>>>,
}

impl<T, H> Clone for VarSetter<T, H> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static, H: DirtyHandle + 'static> Var<T, H> {
    /// Creates a new Var Anchor, returning a tuple of the new Anchor and its setter.
    pub fn new<E: Engine<DirtyHandle = H>>(val: T) -> (Anchor<Self, E>, VarSetter<T, H>) {
        let inner = Rc::new(RefCell::new(VarInner {
            dirty_handle: None,
            val: None,
        }));
        let setter = VarSetter {
            inner: inner.clone(),
        };
        let this = Self { inner, my_val: val };
        (E::mount(this), setter)
    }
}
impl<T: 'static, H: DirtyHandle> VarSetter<T, H> {
    /// Updates the value inside the Var, and indicates to the recomputation graph that
    /// the value has changed.
    pub fn set(&self, val: T) {
        let mut inner = self.inner.borrow_mut();
        inner.val = Some(val);
        if let Some(waker) = &inner.dirty_handle {
            waker.mark_dirty();
        }
    }
}

impl<E: Engine, T: 'static> AnchorInner<E> for Var<T, E::DirtyHandle> {
    type Output = T;
    fn dirty(&mut self, _edge: &<E::AnchorHandle as AnchorHandle>::Token) {
        panic!("somehow an input was dirtied on var; it never has any inputs to dirty")
    }

    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll {
        let mut inner = self.inner.borrow_mut();
        let first_update = inner.dirty_handle.is_none();
        if first_update {
            inner.dirty_handle = Some(ctx.dirty_handle());
        }
        if let Some(new_val) = inner.val.take() {
            self.my_val = new_val;
            Poll::Updated
        } else if first_update {
            Poll::Updated
        } else {
            Poll::Unchanged
        }
    }

    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        _ctx: &mut G,
    ) -> &'out Self::Output
    where
        'slf: 'out,
    {
        &self.my_val
    }
}
