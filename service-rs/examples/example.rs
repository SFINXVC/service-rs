use std::rc::Rc;

use service_rs::ServiceCollection;

trait FirstDep {
    fn say_something(&self);
}

struct FirstDepImpl;
impl FirstDep for FirstDepImpl {
    fn say_something(&self) {
        println!("Hello World! (from FirstDepImpl)");
    }
}

trait SecondDep {
    fn say_something(&self);
}

struct SecondDepImpl;
impl SecondDep for SecondDepImpl {
    fn say_something(&self) {
        println!("Hello World! (from SecondDepImpl)");
    }
}

trait ThirdDep {
    fn debug_to_str(&self) -> String;
    fn say_something(&self);
}

struct ThirdDepImpl {
    first_dep: Rc<Box<dyn FirstDep>>,
    second_dep: Rc<Box<dyn SecondDep>>,
}

impl std::fmt::Debug for ThirdDepImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThirdDepImpl")
            .field("first_dep", &format!("{:p}", self.first_dep))
            .field("second_dep", &format!("{:p}", self.second_dep))
            .finish()
    }
}

impl ThirdDep for ThirdDepImpl {
    fn debug_to_str(&self) -> String {
        format!("{:#?}", self)
    }

    fn say_something(&self) {
        println!("\nHello World! (from ThirdDepImpl)");
        println!("Here, i tried to call first dep and second dep:");
        self.first_dep.say_something();
        self.second_dep.say_something();
    }
}

fn main() {
    let mut collection = ServiceCollection::new();

    collection.add_singleton_boxed::<dyn FirstDep, _>(|_| Box::new(FirstDepImpl));
    collection.add_singleton_boxed::<dyn FirstDep, _>(|_provider| Box::new(FirstDepImpl));
    collection.add_transient_boxed::<dyn SecondDep, _>(|_provider| Box::new(SecondDepImpl));

    collection.add_scoped_boxed::<dyn ThirdDep, _>(|provider| {
        Box::new(ThirdDepImpl {
            first_dep: provider.get_boxed::<dyn FirstDep>().unwrap(),
            second_dep: provider.get_boxed::<dyn SecondDep>().unwrap(),
        })
    });

    // wraps this inside an Rc, so the ScopedServiceProvider can hold the object
    let provider = Rc::new(collection.build());

    let first_get1 = provider.get_boxed::<dyn FirstDep>().unwrap();
    let first_get2 = provider.get_boxed::<dyn FirstDep>().unwrap();
    let first_get3 = provider.get_boxed::<dyn FirstDep>().unwrap();

    let second_get1 = provider.get_boxed::<dyn SecondDep>().unwrap();
    let second_get2 = provider.get_boxed::<dyn SecondDep>().unwrap();
    let second_get3 = provider.get_boxed::<dyn SecondDep>().unwrap();

    // getting a scoped function directly from a ServiceProvider
    // without creating a new scope will make them looks like a singleton
    // since the ProviderService itself act like a global scope.
    let third_get1 = provider.get_boxed::<dyn ThirdDep>().unwrap();
    let third_get2 = provider.get_boxed::<dyn ThirdDep>().unwrap();
    let third_get3 = provider.get_boxed::<dyn ThirdDep>().unwrap();

    // scoped version of third should be located on a different memory location
    let scoped_provider1 = provider.create_scope();
    let third_scoped1_get1 = scoped_provider1.get_boxed::<dyn ThirdDep>().unwrap();
    let third_scoped1_get2 = scoped_provider1.get_boxed::<dyn ThirdDep>().unwrap();
    let third_scoped1_get3 = scoped_provider1.get_boxed::<dyn ThirdDep>().unwrap();

    // creating another scope will also creates a different object
    // and all of it is done on the runtime
    let scoped_provider2 = provider.create_scope();
    let third_scoped2_get1 = scoped_provider2.get_boxed::<dyn ThirdDep>().unwrap();
    let third_scoped2_get2 = scoped_provider2.get_boxed::<dyn ThirdDep>().unwrap();
    let third_scoped2_get3 = scoped_provider2.get_boxed::<dyn ThirdDep>().unwrap();

    // try to get SecondDep from scoped 1 & 2, they both should have different memory address
    let second_scoped1_get1 = scoped_provider1.get_boxed::<dyn SecondDep>().unwrap();
    let second_scoped1_get2 = scoped_provider1.get_boxed::<dyn SecondDep>().unwrap();
    let second_scoped1_get3 = scoped_provider1.get_boxed::<dyn SecondDep>().unwrap();

    let second_scoped2_get1 = scoped_provider2.get_boxed::<dyn SecondDep>().unwrap();
    let second_scoped2_get2 = scoped_provider2.get_boxed::<dyn SecondDep>().unwrap();
    let second_scoped2_get3 = scoped_provider2.get_boxed::<dyn SecondDep>().unwrap();

    println!(
        "FirstDepImpl (singleton) memory address on first get attempt {:p}",
        first_get1
    );
    println!(
        "FirstDepImpl (singleton) memory address on second get attempt {:p}",
        first_get2
    );
    println!(
        "FirstDepImpl (singleton) memory address on third get attempt {:p}",
        first_get3
    );

    println!("\n");

    println!(
        "SecondDepImpl (transient) memory address on first get attempt {:p}",
        second_get1
    );
    println!(
        "SecondDepImpl (transient) memory address on second get attempt {:p}",
        second_get2
    );
    println!(
        "SecondDepImpl (transient) memory address on third get attempt {:p}",
        second_get3
    );

    println!("\n");

    println!(
        "ThirdDepImpl (global scope) memory address on first get attempt {:p}\nwith deps: {}",
        third_get1,
        third_get1.debug_to_str()
    );
    println!(
        "ThirdDepImpl (global scope) memory address on second get attempt {:p}\nwith deps: {}",
        third_get2,
        third_get2.debug_to_str()
    );
    println!(
        "ThirdDepImpl (global scope) memory address on third get attempt {:p}\nwith deps: {}",
        third_get3,
        third_get3.debug_to_str()
    );

    println!("\n");

    println!(
        "ThirdDepImpl (1st scope) memory address on first get attempt {:p}\nwith deps: {}",
        third_scoped1_get1,
        third_scoped1_get1.debug_to_str()
    );
    println!(
        "ThirdDepImpl (1st scope) memory address on second get attempt {:p}\nwith deps: {}",
        third_scoped1_get2,
        third_scoped1_get2.debug_to_str()
    );
    println!(
        "ThirdDepImpl (1st scope) memory address on third get attempt {:p}\nwith deps: {}",
        third_scoped1_get3,
        third_scoped1_get3.debug_to_str()
    );

    println!("\n");

    println!(
        "ThirdDepImpl memory address on scoped second get attempt {:p}\nwith deps: {}",
        third_scoped2_get1,
        third_scoped2_get1.debug_to_str()
    );
    println!(
        "ThirdDepImpl memory address on scoped third get attempt {:p}\nwith deps: {}",
        third_scoped2_get2,
        third_scoped2_get2.debug_to_str()
    );
    println!(
        "ThirdDepImpl memory address on scoped third get attempt {:p}\nwith deps: {}",
        third_scoped2_get3,
        third_scoped2_get3.debug_to_str()
    );

    println!("\n");

    println!(
        "SecondDepImpl (transient via 1st scope) memory address on first get attempt {:p}",
        second_scoped1_get1
    );
    println!(
        "SecondDepImpl (transient via 1st scope) memory address on second get attempt {:p}",
        second_scoped1_get2
    );
    println!(
        "SecondDepImpl (transient via 1st scope) memory address on third get attempt {:p}",
        second_scoped1_get3
    );

    println!("\n");

    println!(
        "SecondDepImpl (transient via 2nd scope) memory address on first get attempt {:p}",
        second_scoped2_get1
    );
    println!(
        "SecondDepImpl (transient via 2nd scope) memory address on second get attempt {:p}",
        second_scoped2_get2
    );
    println!(
        "SecondDepImpl (transient via 2nd scope) memory address on third get attempt {:p}",
        second_scoped2_get3
    );

    println!("\n");

    first_get1.say_something();
    first_get2.say_something();
    first_get3.say_something();

    second_get1.say_something();
    second_get2.say_something();
    second_get3.say_something();

    third_get1.say_something();
    third_get2.say_something();
    third_get3.say_something();

    third_scoped1_get1.say_something();
    third_scoped1_get2.say_something();
    third_scoped1_get3.say_something();

    third_scoped2_get1.say_something();
    third_scoped2_get2.say_something();
    third_scoped2_get3.say_something();
}
