use anchors::singlethread::*;
use anchors::AnchorExt;
use std::cell::RefCell;

thread_local! {
    pub static ENGINE: RefCell<Engine> = RefCell::new(Engine::new());
}

fn main() {
    // important to call ENGINE.with before we create any Anchors, since the engine
    // must have been initialized for an anchor to be created.
    ENGINE.with(|engine| {
        let (foo, set_foo) = anchors::Var::new(1);
        let foo_added = foo.map(|n| n+1);
        println!("{:?}", engine.borrow_mut().get(&foo_added));
    });
}