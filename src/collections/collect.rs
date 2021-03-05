use std::panic::Location;
use crate::expert::{Engine, AnchorInner, UpdateContext, AnchorHandle, OutputContext, Anchor, Poll};

struct VecCollect<T, E: Engine> {
    anchors: Vec<Anchor<T, E>>,
    vals: Option<Vec<T>>,
    location: &'static Location<'static>,
}

impl <I: 'static + Clone, E: Engine> std::iter::FromIterator<Anchor<I, E>> for Anchor<Vec<I>, E> {
    fn from_iter<T>(iter: T) -> Self
        where
            T: IntoIterator<Item = Anchor<I, E>> {
        VecCollect::new(iter.into_iter().collect())
    }
}

impl<T: 'static + Clone, E: Engine> VecCollect<T, E> {
    #[track_caller]
    pub fn new(anchors: Vec<Anchor<T, E>>) -> Anchor<Vec<T>, E> {
        E::mount(Self {
            anchors,
            vals: None,
            location: Location::caller(),
        })
    }
}

impl<T: 'static + Clone, E: Engine> AnchorInner<E>
    for VecCollect<T, E>
{
    type Output = Vec<T>;
    fn dirty(&mut self, _edge: &<E::AnchorHandle as AnchorHandle>::Token) {
        self.vals = None;
    }

    fn poll_updated<G: UpdateContext<Engine = E>>(
        &mut self,
        ctx: &mut G,
    ) -> Poll {
        if self.vals.is_none() {
            let pending_exists = self
                .anchors
                .iter()
                .any(|anchor| ctx.request(anchor, true) == Poll::Pending);
            if pending_exists {
                return Poll::Pending;
            }
            self.vals = Some(
                self.anchors
                    .iter()
                    .map(|anchor| ctx.get(anchor).clone())
                    .collect(),
            )
        }
        Poll::Updated
    }

    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        _ctx: &mut G,
    ) -> &'out Self::Output
    where
        'slf: 'out,
    {
        &self.vals.as_ref().unwrap()
    }

    fn debug_location(&self) -> Option<(&'static str, &'static Location<'static>)> {
        Some(("VecCollect", self.location))
    }
}


#[cfg(test)]
mod test {
    use crate::singlethread::*;
    #[test]
    fn collect() {
        let mut engine = Engine::new();
        let a = Var::new(1);
        let b = Var::new(2);
        let c = Var::new(5);
        let nums: Anchor<Vec<_>> = vec![a.watch(), b.watch(), c.watch()].into_iter().collect();
        let sum: Anchor<usize> = nums.map(|nums| nums.iter().sum());

        assert_eq!(engine.get(&sum), 8);

        a.set(2);
        assert_eq!(engine.get(&sum), 9);

        c.set(1);
        assert_eq!(engine.get(&sum), 5);
    }
}
