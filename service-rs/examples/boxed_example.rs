use service_rs::ServiceCollection;

// Define the dependency as trait
trait FirstDep {
    fn say_something(&self);
}

// Define the implementation of the dependency
struct FirstDepImpl;
impl FirstDep for FirstDepImpl {
    fn say_something(&self) {
        println!("Hello World! (from FirstDepImpl)");
    }
}

// Define the dependency as trait
trait SecondDep {
    fn say_something(&self);
}

// Define the implementation of the dependency
struct SecondDepImpl;
impl SecondDep for SecondDepImpl {
    fn say_something(&self) {
        println!("Hello World! (from SecondDepImpl)");
    }
}

fn main() {
    // Create a new service collection
    let mut collection = ServiceCollection::new();

    // Add the dependencies to the collection

    // singleton: only one instance is created and shared across the application
    collection.add_singleton_boxed::<dyn FirstDep, _>(|_| Ok(Box::new(FirstDepImpl)));

    // transient: a new instance is created each time it is requested
    collection.add_transient_boxed::<dyn SecondDep, _>(|_| Ok(Box::new(SecondDepImpl)));

    // Build the service provider
    let provider = collection.build();

    // Get the dependencies from the provider with "dyn"
    // so we don't have to know the exact type of the implementation details
    // this is done in runtime (might introduce some performance overhead)
    let first_get1 = provider.get_boxed::<dyn FirstDep>().unwrap();
    let first_get2 = provider.get_boxed::<dyn FirstDep>().unwrap();
    let first_get3 = provider.get_boxed::<dyn FirstDep>().unwrap();

    // Get the dependencies for SecondDep too
    let second_get1 = provider.get_boxed::<dyn SecondDep>().unwrap();
    let second_get2 = provider.get_boxed::<dyn SecondDep>().unwrap();
    let second_get3 = provider.get_boxed::<dyn SecondDep>().unwrap();

    // Print the memory address of the dependencies
    // singletons should have the same memory address no matter how many times we get it
    // transients should have different memory addresses each time we get it (it calls the factory each time)
    println!(
        "FirstDepImpl memory address on first get attempt {:p}",
        first_get1
    );
    println!(
        "FirstDepImpl memory address on second get attempt {:p}",
        first_get2
    );
    println!(
        "FirstDepImpl memory address on third get attempt {:p}",
        first_get3
    );

    println!(
        "SecondDepImpl memory address on first get attempt {:p}",
        second_get1
    );
    println!(
        "SecondDepImpl memory address on second get attempt {:p}",
        second_get2
    );
    println!(
        "SecondDepImpl memory address on third get attempt {:p}",
        second_get3
    );

    // Call the dependencies
    first_get1.say_something();
    first_get2.say_something();
    first_get3.say_something();

    second_get1.say_something();
    second_get2.say_something();
    second_get3.say_something();
}
