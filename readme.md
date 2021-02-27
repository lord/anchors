<p align="center">
  <img src="https://user-images.githubusercontent.com/1976330/88240812-1d9b2680-cc3d-11ea-8836-309e96df981d.png" alt="Anchors: Self-Adjusting Computitons in Rust" width="226">
  <br>
  <a href="https://crates.io/crates/anchors"><img src="https://img.shields.io/crates/v/anchors.svg" alt="Crates.io Package"></a> <a href="https://docs.rs/anchors"><img src="https://img.shields.io/badge/docs-docs.rs-success" alt="Docs"></a>
</p>

## Features

- Hybrid graph allows both [Adapton](https://github.com/Adapton/adapton.rust)-style and [Incremental](https://github.com/janestreet/incremental)-style push updates. For more information on the internals, you can view the [accompanying blog post](https://lord.io/blog/2020/spreadsheets/).
- Cloning values in the graph is almost always optional. `map` and `then` closures receive immutable references, and return owned values. Alternatively, a `refmap` closure receives an immutable reference, and returns an immutable reference.
- Still a work in progress, but should be functional (lol) and half-decently fast. Still, expect for there to be major API changes over the next several years.

## Example

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

## Observed nodes

You can tell the engine you'd like a node to be observed:

```rust
engine.mark_observed(&dynamic_name);
```

Now when you request it, it will [avoid traversing the entire graph quite as frequently](https://blog.janestreet.com/seven-implementations-of-incremental/), which is useful when you have a large `Anchor` dependency tree. However, there are some drawbacks:

- any time you `get` *any* `Anchor`, all observed nodes will be brought up to date.
- if one of an observed dependencies is a `then`, nodes requested by it [may be recomputed](https://gist.github.com/khooyp/98abc0e64dc296deaa48), even though they aren't strictly necessary.

## How fast is it?

You can check out the `bench` folder for some microbenchmarks. These are the results of running `stabilize_linear_nodes_simple`, a linear chain of many `map` nodes each adding `1` to some changing input number. Benchmarks run on my Macbook Air (Intel, 2020) against Anchors 0.5.0 `8c9801c`, with `lto = true`.

<table>
  <tr>
    <th>node count</th>
    <th>used `mark_observed`?</th>
    <th>total time to `get` end of chain</th>
    <th>total time / node count</th>
  </tr>

  <tr>
    <td>10</td>
    <td>observed</td>
    <td>[485.48 ns 491.85 ns 498.49 ns]</td>
    <td>49.185 ns</td>
  </tr>

  <tr>
    <td>100</td>
    <td>observed</td>
    <td>[4.1734 us 4.2525 us 4.3345 us]</td>
    <td>42.525 ns</td>
  </tr>

  <tr>
    <td>1000</td>
    <td>observed</td>
    <td>[42.720 us 43.456 us 44.200 us]</td>
    <td>43.456 ns</td>
  </tr>

  <tr>
    <td>10</td>
    <td>unobserved</td>
    <td>[738.02 ns 752.40 ns 767.86 ns]</td>
    <td>75.240 ns</td>
  </tr>


  <tr>
    <td>100</td>
    <td>unobserved</td>
    <td>[6.5952 us 6.7178 us 6.8499 us]</td>
    <td>67.178 ns</td>
  </tr>

  <tr>
    <td>1000</td>
    <td>unobserved</td>
    <td>[74.256 us 75.360 us 76.502 us]</td>
    <td>75.360 ns</td>
  </tr>
</table>

Very roughly, it looks like observed nodes have an overhead of at around `~42-50ns` each, and unobserved nodes around `74-76ns` each. This could be pretty aggressively improved; ideally we could drop these numbers to the `~15ns` per observed node that [Incremental achieves](https://github.com/janestreet/incr_map/blob/master/bench/src/linear.ml).

Also worth mentioning for any incremental program, the slowdowns will probably come from other aspects of the framework that aren't measured with this very simple microbenchmark.

## How fast is it on an M1 mac?

Maybe twice as fast?

<table>
  <tr>
    <th>node count</th>
    <th>used `mark_observed`?</th>
    <th>total time to `get` end of chain</th>
    <th>total time / node count</th>
  </tr>
  <tr>
    <td>10</td>
    <td>observed</td>
    <td>[242.68 ns 242.98 ns 243.37 ns]</td>
    <td>24.30 ns</td>
  </tr>
  <tr>
    <td>100</td>
    <td>observed</td>
    <td>[1.9225 us 1.9232 us 1.9239 us]</td>
    <td>19.232 ns</td>
  </tr>
  <tr>
    <td>1000</td>
    <td>observed</td>
    <td>[20.421 us 20.455 us 20.489 us]</td>
    <td>20.46 ns</td>
  </tr>
  <tr>
    <td>10</td>
    <td>unobserved</td>
    <td>[354.05 ns 354.21 ns 354.37 ns]</td>
    <td>35.42</td>
  </tr>
  <tr>
    <td>100</td>
    <td>unobserved</td>
    <td>[3.3810 us 3.3825 us 3.3841 us]</td>
    <td>33.83 ns</td>
  </tr>
  <tr>
    <td>1000</td>
    <td>unobserved</td>
    <td>[41.429 us 41.536 us 41.642 us]</td>
    <td>41.54 ns</td>
  </tr>
</table>

## See Also

- https://github.com/Adapton/adapton.rust
- https://github.com/janestreet/incremental
- https://github.com/observablehq/runtime
- https://github.com/salsa-rs/salsa
- https://github.com/MaterializeInc/materialize
