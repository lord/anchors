# anchors

Another incremental computation library, for Rust

## Features

- lots of bugs. probably major performance problems. incremental computation is hard and i am dumb
- hybrid graph allows both [Adapton](https://github.com/Adapton/adapton.rust)-style and [Incremental](https://github.com/janestreet/incremental)-style push updates
- [still have to implement] minimal allocations through the use of [generational-arena](https://github.com/fitzgen/generational-arena)
- [still have to implement] maybe multithreading engine at some point in the future
- [still have to implement] values that change over time, similar to [observablehq generators](https://observablehq.com/@observablehq/introduction-to-generators)

## example

```rust
// example
use crate::{singlethread::Engine, AnchorExt, Var};
let mut engine = Engine::new();

// create a couple `Var`s
let (my_name, my_name_updater) = Var::new("Bob".to_string());
let (my_unread, my_unread_updater) = Var::new(999usize);

// `my_name` is a `Var`, our first type of `Anchor`. we can pull an `Anchor`'s value out with our `engine`:
assert_eq!(&engine.get(&my_name), "Bob");
assert_eq!(engine.get(&my_unread), 999);

// we can create a new `Anchor` from another one using `map`. The function won't actually run until absolutely necessary.
// also feel free to clone an `Anchor` — the clones will all refer to the same inner state
let my_greeting = my_name.clone().map(|name| {
    println!("calculating name!");
    format!("Hello, {}!", name)
});
assert_eq!(engine.get(&my_greeting), "Hello, Bob!"); // prints "calculating name!"

// we can update a `Var` with its updater. values are cached unless one of its dependencies changes
assert_eq!(engine.get(&my_greeting), "Hello, Bob!"); // doesn't print anything
my_name_updater.set("Robo".to_string());
assert_eq!(engine.get(&my_greeting), "Hello, Robo!"); // prints "calculating name!"

// a `map` can take values from multiple `Anchor`s. just use tuples:
let header = (&my_greeting, &my_unread)
    .map(|greeting, unread| format!("{} You have {} new messages.", greeting, unread));
assert_eq!(
    engine.get(&header),
    "Hello, Robo! You have 999 new messages."
);

// just like a future, you can dynamically decide which `Anchor` to use with `then`:
let (insulting_name, _) = Var::new("Lazybum".to_string());
let dynamic_name = my_unread.then(move |unread| {
    // only use the user's real name if the have less than 100 messages in their inbox
    if *unread < 100 {
        my_name.clone()
    } else {
        insulting_name.clone()
    }
});
assert_eq!(engine.get(&dynamic_name), "Lazybum");
my_unread_updater.set(50);
assert_eq!(engine.get(&dynamic_name), "Robo");
```

## observed nodes

You can tell the engine you'd like a node to be observed:

```rust
engine.mark_observed(&dynamic_name);
```

Now when you request it, it will [avoid traversing the entire graph quite as frequently](https://blog.janestreet.com/seven-implementations-of-incremental/), which is useful when you have a large `Anchor` dependency tree. However, there are some drawbacks:

- any time you `get` *any* `Anchor`, all observed nodes will be brought up to date.
- if one of an observed dependencies is a `then`, nodes requested by it [may be recomputed](https://gist.github.com/khooyp/98abc0e64dc296deaa48), even though they aren't strictly necessary.

## later

- if a dirty value recomputes and it's the same, the graph should be able to 'recover' and stop recomputation?
- speed up by directly calculating a `get` if it is not necessary
- could optimize a lot (and also maybe do async nodes properly) if context is just a struct like async does it, so that we don't need higher kinded types to do lifetimes on that stuff correctly
- separate 'dirty' vs 'changed', a node should get a notification for when a dependency is dirty but then when it actually goes to retrieve should be informed if the value has changed since the last get. this is important if a node requests its parents as clean dependencies instead of necessary dependencies
- async nodes, unfortunately kept implementing things that needed either higher kinded types, or lots of cloning
- multithreading
- actual speed, figuring out how to reduce allocations
- serializing cached computations

## see also

- https://github.com/Adapton/adapton.rust
- https://github.com/janestreet/incremental
- https://github.com/observablehq/runtime
- https://github.com/salsa-rs/salsa
