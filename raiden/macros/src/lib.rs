//! Provides simple macros for type conversions.
use proc_macro::TokenStream;
use quote::quote;
use syn::{
	parse_macro_input,
	DeriveInput,
};

/// Derive macro to convert input into an Event.
#[proc_macro_derive(IntoEvent)]
pub fn into_event(input: TokenStream) -> TokenStream {
	// Parse the input tokens into a syntax tree
	let input = parse_macro_input!(input as DeriveInput);
	let name = input.ident;

	let expanded = quote! {
		impl Into<Event> for #name {
			fn into(self) -> Event {
				Event::#name(self)
			}
		}
	};

	TokenStream::from(expanded)
}

/// Derive macro to convert input into a state change.
#[proc_macro_derive(IntoStateChange)]
pub fn into_state_change(input: TokenStream) -> TokenStream {
	// Parse the input tokens into a syntax tree
	let input = parse_macro_input!(input as DeriveInput);
	let name = input.ident;

	let expanded = quote! {
		impl Into<StateChange> for #name {
			fn into(self) -> StateChange {
				StateChange::#name(self)
			}
		}
	};

	TokenStream::from(expanded)
}
