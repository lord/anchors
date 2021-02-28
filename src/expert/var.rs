use super::{
    Anchor, AnchorHandle, AnchorInner, DirtyHandle, Engine, OutputContext, Poll, UpdateContext,
};
use std::cell::RefCell;
use std::rc::Rc;

/// An Anchor type for values that are mutated by calling a setter function from outside of the Anchors recomputation graph.
struct VarAnchor<T, E: Engine> {
    inner: Rc<RefCell<VarShared<T, E>>>,
    val: Rc<T>,
}

#[derive(Clone)]
struct VarShared<T, E: Engine> {
    dirty_handle: Option<E::DirtyHandle>,
    val: Rc<T>,
    value_changed: bool,
}

/// A setter that can update values inside an associated `VarAnchor`.
pub struct Var<T, E: Engine> {
    inner: Rc<RefCell<VarShared<T, E>>>,
    anchor: Anchor<T, E>,
}

impl<T, E: Engine> Clone for Var<T, E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            anchor: self.anchor.clone(),
        }
    }
}

impl<T: 'static, E: Engine> Var<T, E> {
    /// Creates a new Var
    pub fn new(val: T) -> Var<T, E> {
        let val = Rc::new(val);
        let inner = Rc::new(RefCell::new(VarShared {
            dirty_handle: None,
            val: val.clone(),
            value_changed: true,
        }));
        Var {
            inner: inner.clone(),
            anchor: E::mount(VarAnchor { inner, val }),
        }
    }

    /// Updates the value inside the VarAnchor, and indicates to the recomputation graph that
    /// the value has changed.
    pub fn set(&self, val: T) {
        let mut inner = self.inner.borrow_mut();
        inner.val = Rc::new(val);
        if let Some(waker) = &inner.dirty_handle {
            waker.mark_dirty();
        }
        inner.value_changed = true;
    }

    /// Retrieves the last value set
    pub fn get(&self) -> Rc<T> {
        self.inner.borrow().val.clone()
    }

    pub fn watch(&self) -> Anchor<T, E> {
        self.anchor.clone()
    }
}

impl<E: Engine, T: 'static> AnchorInner<E> for VarAnchor<T, E> {
    type Output = T;
    fn dirty(&mut self, _edge: &<E::AnchorHandle as AnchorHandle>::Token) {
        panic!("somehow an input was dirtied on VarAnchor; it never has any inputs to dirty")
    }

    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll {
        let mut inner = self.inner.borrow_mut();
        let first_update = inner.dirty_handle.is_none();
        if first_update {
            inner.dirty_handle = Some(ctx.dirty_handle());
        }
        let res = if inner.value_changed {
            self.val = inner.val.clone();
            Poll::Updated
        } else {
            Poll::Unchanged
        };
        inner.value_changed = false;
        res
    }

    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        _ctx: &mut G,
    ) -> &'out Self::Output
    where
        'slf: 'out,
    {
        &self.val
    }
}
