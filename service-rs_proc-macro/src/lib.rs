use proc_macro::TokenStream;
use syn::{Data, DeriveInput};

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
