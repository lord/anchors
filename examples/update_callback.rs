use anchors::expert::{AnchorExt, Var};
use anchors::singlethread::*;
use std::cell::RefCell;

fn main() {
    let mut engine = Engine::new();
    let (cat_count, set_cat_count) = Var::new(1);
    let (dog_count, set_dog_count) = Var::new(1);
    let (fish_count, set_fish_count) = Var::new(1);
    let total_mammals = (&cat_count, &dog_count).map(|cats, dogs| cats + dogs);
    let total_animals = (&total_mammals, &fish_count).map(|mammals, fish| mammals + fish);
    let mammal_callback = total_mammals.map(|total_mammals| println!("mammals updated: {:?}", total_mammals));
    let animal_callback = total_animals.map(|total_animals| println!("animals updated: {:?}", total_animals));
    engine.mark_observed(&mammal_callback);
    engine.mark_observed(&animal_callback);

    println!("stabilizing...");
    engine.stabilize();

    set_cat_count.set(2);
    set_dog_count.set(2);
    println!("stabilizing...");
    engine.stabilize();

    set_fish_count.set(2);
    println!("stabilizing...");
    engine.stabilize();
}
