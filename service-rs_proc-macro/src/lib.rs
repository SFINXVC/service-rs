use proc_macro::TokenStream;
use syn::{Data, DeriveInput};

/// Derives the `Injectable` trait for automatic dependency injection.
///
/// This procedural macro generates a constructor and implements the `InjectableExtension` trait,
/// enabling automatic dependency resolution through the service container.
///
/// # Requirements
///
/// - All fields must be wrapped in `Arc<T>` where `T` is a registered service type
/// - The struct must have named fields or be a unit struct
/// - All dependency types must be resolvable from the service provider
///
/// # Generated Code
///
/// The macro generates:
/// 1. A `new` constructor that accepts all dependencies as parameters
/// 2. An `InjectableExtension` implementation with a factory function that:
///    - Resolves each dependency from the service provider context
///    - Constructs the type with resolved dependencies
///    - Returns a boxed instance
///
/// # Examples
///
/// ## Basic Usage with Dependencies
///
/// ```rust,ignore
/// use service_rs::{Injectable, ServiceCollection};
/// use std::sync::Arc;
///
/// // Mock types for demonstration
/// struct ConnectionPool;
/// struct Logger;
///
/// #[derive(Injectable)]
/// struct DatabaseService {
///     connection_pool: Arc<ConnectionPool>,
///     logger: Arc<Logger>,
/// }
///
/// // Generated code (conceptual):
/// // impl DatabaseService {
/// //     pub fn new(connection_pool: Arc<ConnectionPool>, logger: Arc<Logger>) -> Self {
/// //         Self { connection_pool, logger }
/// //     }
/// // }
/// ```
///
/// ## Unit Struct (No Dependencies)
///
/// ```rust,ignore
/// use service_rs::Injectable;
///
/// #[derive(Injectable)]
/// struct SimpleService;
///
/// // Generated code (conceptual):
/// // impl SimpleService {
/// //     pub fn new() -> Self {
/// //         Self
/// //     }
/// // }
/// ```
///
/// ## Registration with Service Collection
///
/// ```rust,ignore
/// use service_rs::{Injectable, ServiceCollection};
/// use std::sync::Arc;
///
/// #[derive(Injectable)]
/// struct MyService {
///     dependency: Arc<i32>,
/// }
///
/// let collection = ServiceCollection::new()
///     .add_singleton_with_factory::<i32, _, _>(|_| async {
///         Ok(Box::new(42) as Box<dyn std::any::Any + Send + Sync>)
///     })
///     .add_singleton::<MyService>(); // Uses Injectable
/// ```
///
/// # Compile-Time Errors
///
/// The macro will produce compile errors if:
///
/// - Fields are not wrapped in `Arc<T>`:
///   ```compile_fail
///   #[derive(Injectable)]
///   struct BadService {
///       field: String, // Error: must be Arc<String>
///   }
///   ```
///
/// - Used on enums:
///   ```compile_fail
///   #[derive(Injectable)]
///   enum BadEnum { Variant } // Error: Injectable only works with structs
///   ```
///
/// - Used with unnamed fields:
///   ```compile_fail
///   #[derive(Injectable)]
///   struct BadService(Arc<String>); // Error: must use named fields
///   ```
///
/// # Runtime Behavior
///
/// When a service is resolved:
/// 1. The factory function is called with a `ServiceProviderContext`
/// 2. Each dependency is resolved asynchronously via `ctx.get::<T>()`
/// 3. If any dependency fails to resolve, an error is propagated
/// 4. On success, the constructor is called with all resolved dependencies
///
/// # Performance Notes
///
/// - Dependencies are resolved lazily when the service is first requested
/// - `Arc<T>` provides efficient reference counting for shared ownership
/// - Async resolution allows for non-blocking initialization
#[proc_macro_derive(Injectable)]
pub fn derive_injectable(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = match input.data {
        Data::Struct(data_struct) => match data_struct.fields {
            syn::Fields::Named(fields_named) => {
                let mut params = Vec::new();
                let mut inits = Vec::new();
                let mut inner_types = Vec::new();

                for field in fields_named.named {
                    let ident = field.ident.unwrap();
                    let ty = field.ty;

                    let inner_ty = match &ty {
                        syn::Type::Path(type_path) => {
                            if let Some(seg) = type_path.path.segments.first() {
                                if seg.ident == "Arc" {
                                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments
                                    {
                                        if let Some(syn::GenericArgument::Type(inner)) =
                                            args.args.first()
                                        {
                                            Some(inner.clone())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if inner_ty.is_none() {
                        return quote::quote! {
                                compile_error!(concat!("Field `", stringify!(#ident), "` in `", stringify!(#name), "` must be wrapped in Arc<T>!. But if you want to make this object Injectable, consider using `ServiceCollection::add_singleton_with_factory`, `ServiceCollection::add_scoped_with_factory` or `ServiceCollection::add_transient_with_factory` instead."));
                            }.into();
                    }

                    params.push(quote::quote! { #ident: #ty });
                    inits.push(quote::quote! { #ident });
                    inner_types.push(inner_ty.unwrap());
                }

                let field_idents = inits.clone();

                quote::quote! {
                    impl #name {
                        pub fn new(#(#params),*) -> Self {
                            Self {
                                #(#inits),*
                            }
                        }
                    }

                    impl service_rs::InjectableExtension for #name {
                        fn create_factory() -> service_rs::ServiceFactory {
                            Box::new(|ctx: service_rs::ServiceProviderContext| {
                                Box::pin(async move {
                                    #(
                                        let #field_idents = ctx.get::<#inner_types>().await
                                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                                    )*

                                    Ok(Box::new(#name::new(#(#field_idents),*)) as Box<dyn std::any::Any + Send + Sync>)
                                })
                            })
                        }
                    }
                }
            }
            syn::Fields::Unit => {
                quote::quote! {
                    impl #name {
                        pub fn new() -> Self {
                            Self
                        }
                    }

                    impl service_rs::InjectableExtension for #name {
                        fn create_factory() -> service_rs::ServiceFactory {
                            Box::new(|_ctx: service_rs::ServiceProviderContext| {
                                Box::pin(async move {
                                    Ok(Box::new(#name::new()) as Box<dyn std::any::Any + Send + Sync>)
                                })
                            })
                        }
                    }
                }
            }
            _ => quote::quote! {
                compile_error!("Injectable can only be used with named fields or unit struct!");
            },
        },
        _ => quote::quote! {
            compile_error!("Injectable can only be used with struct!");
        },
    };

    expanded.into()
}
