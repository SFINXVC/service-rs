use std::sync::Arc;

use service_rs::{Injectable, ServiceCollection};

struct SomeStringWrapper(String);

impl std::fmt::Display for SomeStringWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

struct TestDep(i32);

impl std::fmt::Display for TestDep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Injectable)]
struct DependingDeps {
    test_dep: Arc<TestDep>,
}

impl std::fmt::Display for DependingDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hello from DependingDeps (via std::fmt::Display)")
    }
}

impl DependingDeps {
    pub fn test_dep(&self) -> &Arc<TestDep> {
        &self.test_dep
    }
}

trait SomeInterface: Send + Sync {
    fn do_something(&self);
}

#[derive(Injectable)]
struct SomeInterfaceImpl;

impl SomeInterface for SomeInterfaceImpl {
    fn do_something(&self) {
        println!("Doing something");
    }
}

#[tokio::main]
async fn main() {
    let collection = ServiceCollection::new()
        .add_singleton_with_factory::<i32, _, _>(|_| async {
            Ok(Box::new(42) as Box<dyn std::any::Any + Send + Sync>)
        })
        .add_transient_with_factory::<SomeStringWrapper, _, _>(|_| async {
            Ok(Box::new(SomeStringWrapper("Hello".to_string()))
                as Box<dyn std::any::Any + Send + Sync>)
        })
        .add_scoped_with_factory::<TestDep, _, _>(|_| async {
            Ok(Box::new(TestDep(42)) as Box<dyn std::any::Any + Send + Sync>)
        })
        .add_scoped::<DependingDeps>()
        .add_scoped_interface::<dyn SomeInterface, SomeInterfaceImpl>();

    println!("Successfully registered {} services", collection.len());

    let provider = collection.build();

    println!("Successfully built ServiceProvider");

    {
        match provider.get::<i32>().await {
            Ok(first) => println!(
                "(first get attempt) Service with type {} ({:p}) value is {}",
                std::any::type_name::<i32>(),
                Arc::as_ptr(&first),
                first
            ),
            Err(e) => println!(
                "(first get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<i32>(),
                e
            ),
        }

        match provider.get::<i32>().await {
            Ok(second) => println!(
                "(second get attempt) Service with type {} ({:p}) value is {}",
                std::any::type_name::<i32>(),
                Arc::as_ptr(&second),
                second
            ),
            Err(e) => println!(
                "(second get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<i32>(),
                e
            ),
        }
    }

    {
        match provider.get::<SomeStringWrapper>().await {
            Ok(first) => println!(
                "(first get attempt) Service with type {} ({:p}) value is {}",
                std::any::type_name::<SomeStringWrapper>(),
                Arc::as_ptr(&first),
                first
            ),
            Err(e) => println!(
                "(first get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<SomeStringWrapper>(),
                e
            ),
        }

        match provider.get::<SomeStringWrapper>().await {
            Ok(second) => println!(
                "(second get attempt) Service with type {} ({:p}) value is {}",
                std::any::type_name::<SomeStringWrapper>(),
                Arc::as_ptr(&second),
                second
            ),
            Err(e) => println!(
                "(second get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<SomeStringWrapper>(),
                e
            ),
        }
    }

    {
        match provider.get::<TestDep>().await {
            Ok(first) => println!(
                "(first get attempt) Service with type {} ({:p}) value is {}",
                std::any::type_name::<TestDep>(),
                Arc::as_ptr(&first),
                first
            ),
            Err(e) => println!(
                "(first get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<TestDep>(),
                e
            ),
        }

        match provider.get::<TestDep>().await {
            Ok(second) => println!(
                "(second get attempt) Service with type {} ({:p}) value is {}",
                std::any::type_name::<TestDep>(),
                Arc::as_ptr(&second),
                second
            ),
            Err(e) => println!(
                "(second get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<TestDep>(),
                e
            ),
        }
    }

    {
        let scope = provider.create_scope();

        match scope.get::<TestDep>().await {
            Ok(first) => println!(
                "(first get attempt) Service with type {} ({:p}) value is {}",
                std::any::type_name::<TestDep>(),
                Arc::as_ptr(&first),
                first
            ),
            Err(e) => println!(
                "(first get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<TestDep>(),
                e
            ),
        }

        match scope.get::<TestDep>().await {
            Ok(second) => println!(
                "(second get attempt) Service with type {} ({:p}) value is {}",
                std::any::type_name::<TestDep>(),
                Arc::as_ptr(&second),
                second
            ),
            Err(e) => println!(
                "(second get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<TestDep>(),
                e
            ),
        }

        match scope.get::<DependingDeps>().await {
            Ok(first) => println!(
                "(first get attempt) Service with type {} ({:p}) resolved successfully with test_dep at {:p}",
                std::any::type_name::<DependingDeps>(),
                Arc::as_ptr(&first),
                Arc::as_ptr(first.test_dep())
            ),
            Err(e) => println!(
                "(first get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<DependingDeps>(),
                e
            ),
        }

        match scope.get::<DependingDeps>().await {
            Ok(second) => println!(
                "(second get attempt) Service with type {} ({:p}) resolved successfully with test_dep at {:p}",
                std::any::type_name::<DependingDeps>(),
                Arc::as_ptr(&second),
                Arc::as_ptr(second.test_dep())
            ),
            Err(e) => println!(
                "(second get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<DependingDeps>(),
                e
            ),
        }
    }

    {
        match provider
            .create_scope()
            .get::<Box<dyn SomeInterface>>()
            .await
        {
            Ok(first) => {
                println!(
                    "(first get attempt) Service with type {} ({:p}) resolved successfully",
                    std::any::type_name::<Box<dyn SomeInterface>>(),
                    Arc::as_ptr(&first)
                );
                first.do_something();
            }
            Err(e) => println!(
                "(first get attempt) Failed to get service with type {}: {}",
                std::any::type_name::<Box<dyn SomeInterface>>(),
                e
            ),
        }
    }
}
