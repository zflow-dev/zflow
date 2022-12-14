use proc_macro::TokenStream;
use quote::quote;

use syn::parse::Parser;
use syn::{parse_macro_input, ItemStruct, DeriveInput};


#[proc_macro_attribute]
pub fn event_handler_attributes(_: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_struct = parse_macro_input!(input as ItemStruct);
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        let parsed_field = syn::Field::parse_named.parse2(quote! { pub event_counter: usize}.into());
        match parsed_field {
            Ok(x) => fields.named.push(x),
            Err(err) => panic!("event_handler_attributes: {:?}", err),
        }

        let parsed_field2 = syn::Field::parse_named.parse2(
            quote! { pub event_recs: HashMap<String, Vec<EventActor<Self>>>},
        );
        match parsed_field2 {
            Ok(x) => fields.named.push(x),
            Err(err) => panic!("event_handler_attributes: {:?}", err),
        }
    }

    return quote! {
        #item_struct
    }
    .into();
}

#[proc_macro_derive(EventHandler)]
pub fn event_handler_trait(input: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(input as DeriveInput);

    let name = item_struct.clone().ident;

    let generics = item_struct.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // panic!("{:?}", impl_generics);

    let expanded = quote! {
        impl #impl_generics EventSignal for #name #ty_generics #where_clause {
            fn emit(&mut self, name:&str, data: &dyn Any) {
                // let d = Box::new(data);
                if self.event_recs.clone().contains_key(name) {
                    if let Some(recs) = self.event_recs.get_mut(name){
                        let mut count = recs.clone().len();
                        for i in 0..count{
                            let r = recs[i];
                            if r.once {
                                recs.remove(i);
                                count -=1;
                            }
                            block_on(r.callback.lock())(self, data);
                        }
                        self.event_recs.insert(name.to_string(), recs.clone());
                    }
                    
                    // if let Some(recs) = self.event_recs.clone().get_mut(name) {
                    //     let mut count = recs.clone().len();
                    //     for i in 0..count{
                    //         if let Some(r) = recs.get_mut(i){
                    //             if r.1 {
                    //                 recs.remove(i);
                    //                 count -=1;
                    //             }
                    //             self.event_recs.insert(name.to_string(), recs.clone());
                    //             r.0(self, data);
                    //         }
                            
                    //     }
                    // }
                }

            }

            fn connect<F>(
                &mut self,
                name: &str,
                rec: F,
                once: bool,
            ) where F: FnMut(&mut Self, &dyn Any) -> () + 'static {
                self.event_counter += 1;
                // let i: usize = self.event_counter;

                if !self.event_recs.contains_key(name) {
                    self.event_recs.insert(name.to_string(), Vec::new());
                }
                self.event_recs.get_mut(name).unwrap().push(EventActor{once, callback:Arc::new(Mutex::new(rec))});
                // (i, once)
            }

            fn disconnect(&mut self, name:&str) {
                self.event_recs.remove(name);
            }

            fn has_event(&self, name:&str) -> bool {
                self.event_recs.contains_key(name)
            }
        }
    };

    TokenStream::from(expanded)
}

