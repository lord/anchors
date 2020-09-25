use super::{Anchor, AnchorInner, Engine};
use std::panic::Location;

mod cutoff;
mod map;
mod refmap;
mod then;

/// A trait automatically implemented for all Anchors.
/// You'll likely want to `use` this trait in most of your programs, since it can create many
/// useful Anchors that derive their output incrementally from some other Anchors.
///
/// AnchorExt is also implemented for all tuples of up to 9 Anchor references. For example:
///
/// ```
/// use anchors::{singlethread::Engine, Constant, AnchorExt};
/// let mut engine = Engine::new();
/// let a = Constant::new(1);
/// let b = Constant::new(2);
/// let c = Constant::new(3);
///
/// // here we use AnchorExt to map three values together:
/// let res = (&a, &b, &c).map(|a_val, b_val, c_val| *a_val+*b_val+*c_val);
///
/// assert_eq!(6, engine.get(&res));
/// ```
pub trait AnchorExt<E: Engine>: Sized {
    type Target;
    fn map<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        map::Map<Self::Target, F, Out>: AnchorInner<E, Output = Out>;

    fn refmap<F, Out>(self, _f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        refmap::RefMap<Self::Target, F>: AnchorInner<E, Output = Out>;

    fn then<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        F: 'static,
        Out: 'static,
        then::Then<Self::Target, Out, F, E>: AnchorInner<E, Output = Out>;

    fn cutoff<F, Out>(self, _f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        cutoff::Cutoff<Self::Target, F>: AnchorInner<E, Output = Out>;
}

pub trait AnchorSplit<E: Engine>: Sized {
    type Target;
    fn split(&self) -> Self::Target;
}

impl<O1, E> AnchorExt<E> for &Anchor<O1, E>
where
    O1: 'static,
    E: Engine,
{
    type Target = (Anchor<O1, E>,);

    #[track_caller]
    fn map<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        map::Map<Self::Target, F, Out>: AnchorInner<E, Output = Out>,
    {
        E::mount(map::Map {
            anchors: (self.clone(),),
            f,
            output: None,
            output_stale: true,
            location: Location::caller(),
        })
    }

    #[track_caller]
    fn then<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        F: 'static,
        Out: 'static,
        then::Then<Self::Target, Out, F, E>: AnchorInner<E, Output = Out>,
    {
        E::mount(then::Then {
            anchors: (self.clone(),),
            f,
            f_anchor: None,
            location: Location::caller(),
            lhs_stale: true,
        })
    }

    #[track_caller]
    fn refmap<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        refmap::RefMap<Self::Target, F>: AnchorInner<E, Output = Out>,
    {
        E::mount(refmap::RefMap {
            anchors: (self.clone(),),
            f,
            location: Location::caller(),
        })
    }

    #[track_caller]
    fn cutoff<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        cutoff::Cutoff<Self::Target, F>: AnchorInner<E, Output = Out>,
    {
        E::mount(cutoff::Cutoff {
            anchors: (self.clone(),),
            f,
            location: Location::caller(),
        })
    }
}

macro_rules! impl_tuple_ext {
    ($([$output_type:ident, $num:tt])+) => {
        impl <$($output_type,)+ E> AnchorSplit<E> for Anchor<($($output_type,)+), E>
        where
            $(
                $output_type: Clone + PartialEq + 'static,
            )+
            E: Engine,
        {
            type Target = ($(Anchor<$output_type, E>,)+);

            fn split(&self) -> Self::Target {
                ($(
                    self.refmap(|v| &v.$num),
                )+)
            }
        }

        impl<$($output_type,)+ E> AnchorExt<E> for ($(&Anchor<$output_type, E>,)+)
        where
            $(
                $output_type: 'static,
            )+
            E: Engine,
        {
            type Target = ($(Anchor<$output_type, E>,)+);

            #[track_caller]
            fn map<F, Out>(self, f: F) -> Anchor<Out, E>
            where
                Out: 'static,
                F: 'static,
                map::Map<Self::Target, F, Out>: AnchorInner<E, Output=Out>,
            {
                E::mount(map::Map {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    output: None,
                    output_stale: true,
                    location: Location::caller(),
                })
            }

            #[track_caller]
            fn then<F, Out>(self, f: F) -> Anchor<Out, E>
            where
                F: 'static,
                Out: 'static,
                then::Then<Self::Target, Out, F, E>: AnchorInner<E, Output=Out>,
            {
                E::mount(then::Then {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    f_anchor: None,
                    location: Location::caller(),
                    lhs_stale: true,
                })
            }

            #[track_caller]
            fn refmap<F, Out>(self, f: F) -> Anchor<Out, E>
            where
                Out: 'static,
                F: 'static,
                refmap::RefMap<Self::Target, F>: AnchorInner<E, Output = Out>,
            {
                E::mount(refmap::RefMap {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    location: Location::caller(),
                })
            }

            #[track_caller]
            fn cutoff<F, Out>(self, f: F) -> Anchor<Out, E>
            where
                Out: 'static,
                F: 'static,
                cutoff::Cutoff<Self::Target, F>: AnchorInner<E, Output = Out>,
            {
                E::mount(cutoff::Cutoff {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    location: Location::caller(),
                })
            }
        }
    }
}

impl_tuple_ext! {
    [O0, 0]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
    [O7, 7]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
    [O7, 7]
    [O8, 8]
}
