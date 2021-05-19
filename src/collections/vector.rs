use im::Vector;

use crate::expert::{
    Anchor, AnchorHandle, AnchorInner, Engine, OutputContext, Poll, UpdateContext,
};
use std::panic::Location;

impl<I: 'static + Clone, E: Engine> std::iter::FromIterator<Anchor<I, E>> for Anchor<Vector<I>, E> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Anchor<I, E>>,
    {
        VectorCollect::new(iter.into_iter().collect())
    }
}

impl<'a, I: 'static + Clone, E: Engine> std::iter::FromIterator<&'a Anchor<I, E>>
    for Anchor<Vector<I>, E>
{
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = &'a Anchor<I, E>>,
    {
        VectorCollect::new(iter.into_iter().cloned().collect())
    }
}

struct VectorCollect<T, E: Engine> {
    anchors: Vector<Anchor<T, E>>,
    vals: Option<Vector<T>>,
    location: &'static Location<'static>,
}

impl<T: 'static + Clone, E: Engine> VectorCollect<T, E> {
    #[track_caller]
    pub fn new(anchors: Vector<Anchor<T, E>>) -> Anchor<Vector<T>, E> {
        E::mount(Self {
            anchors,
            vals: None,
            location: Location::caller(),
        })
    }
}

impl<T: 'static + Clone, E: Engine> AnchorInner<E> for VectorCollect<T, E> {
    type Output = Vector<T>;
    fn dirty(&mut self, _edge: &<E::AnchorHandle as AnchorHandle>::Token) {
        self.vals = None;
    }

    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll {
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
        Some(("VectorCollect", self.location))
    }
}

#[cfg(test)]
mod test {
    use crate::singlethread::*;
    use im::vector;
    use im::Vector;

    #[test]
    fn collect() {
        let mut engine = Engine::new();
        let a = Var::new(1);
        let b = Var::new(2);
        let c = Var::new(5);
        let nums: Anchor<Vector<_>> = vector![a.watch(), b.watch(), c.watch()]
            .into_iter()
            .collect();
        let sum: Anchor<usize> = nums.map(|nums| nums.iter().sum());
        let ns: Anchor<usize> = nums.map(|nums: &Vector<_>| nums.len());

        assert_eq!(engine.get(&sum), 8);

        a.set(2);
        assert_eq!(engine.get(&sum), 9);

        c.set(1);
        assert_eq!(engine.get(&sum), 5);
        println!("ns {}", engine.get(&ns));
    }
}
