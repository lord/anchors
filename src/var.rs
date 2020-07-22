use super::{Anchor, AnchorInner, DirtyHandle, Engine, OutputContext, UpdateContext};
use std::cell::RefCell;
use std::rc::Rc;
use std::task::Poll;

pub struct Var<T, H> {
    inner: Rc<RefCell<VarInner<T, H>>>,
    my_val: T,
}

#[derive(Clone)]
struct VarInner<T, H> {
    dirty_handle: Option<H>,
    val: Option<T>,
}

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
    pub fn new<E: Engine<DirtyHandle = H>>(val: T) -> (Anchor<T, E>, VarSetter<T, H>) {
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
    pub fn set(&self, val: T) {
        let mut inner = self.inner.borrow_mut();
        inner.val = Some(val);
        if let Some(waker) = &inner.dirty_handle {
            waker.mark_dirty();
        }
    }
}

impl<E: Engine, T: 'static> AnchorInner<E> for Var<T, E::DirtyHandle> {
    type Output<'a> = &'a T;
    fn dirty(&mut self, _edge: &E::AnchorData) {
        panic!("somehow an input was dirtied on var; it never has any inputs to dirty")
    }

    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll<bool> {
        let mut inner = self.inner.borrow_mut();
        let first_update = inner.dirty_handle.is_none();
        if first_update {
            inner.dirty_handle = Some(ctx.dirty_handle());
        }
        if let Some(new_val) = inner.val.take() {
            self.my_val = new_val;
            Poll::Ready(true)
        } else {
            // always true if this is the first update
            Poll::Ready(first_update)
        }
    }

    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        _ctx: &mut G,
    ) -> Self::Output<'out>
    where
        'slf: 'out,
    {
        &self.my_val
    }
}
