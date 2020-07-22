use super::{Anchor, AnchorInner, Engine};
use std::panic::Location;

mod map;
mod refmap;
mod then;

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
        refmap::RefMap<Self::Target, F>: AnchorInner<E, Output = Out>,
    {
        unimplemented!()
    }

    fn then<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        F: 'static,
        Out: 'static,
        then::Then<Self::Target, Out, F, E>: AnchorInner<E, Output = Out>;
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
