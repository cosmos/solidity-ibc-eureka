//! Procedural macros for IBC applications on Solana
//!
//! This crate provides:
//! - `#[ibc_app]` - Validates IBC app callback implementations
//! - `discriminator!` - Computes Anchor discriminators at compile time

use proc_macro::TokenStream;
use quote::quote;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use syn::{parse_macro_input, FnArg, ItemFn, ItemMod, LitStr, ReturnType, Type, TypePath};

// ============================================================================
// Constants & Types
// ============================================================================

/// Configuration for each IBC callback
#[derive(Debug, Clone)]
struct CallbackConfig {
    /// The function name
    name: &'static str,
    /// Expected message type name
    msg_type: &'static str,
    /// Expected return types (multiple allowed for flexibility)
    return_types: &'static [ReturnTypeExpectation],
    /// Discriminator name (for compile-time checks)
    discriminator_name: &'static str,
}

#[derive(Debug, Clone)]
enum ReturnTypeExpectation {
    ResultUnit,
    ResultVecU8,
}

impl CallbackConfig {
    const fn new(
        name: &'static str,
        msg_type: &'static str,
        return_types: &'static [ReturnTypeExpectation],
        discriminator_name: &'static str,
    ) -> Self {
        Self {
            name,
            msg_type,
            return_types,
            discriminator_name,
        }
    }
}

/// IBC callback configurations
const IBC_CALLBACKS: &[CallbackConfig] = &[
    CallbackConfig::new(
        "on_recv_packet",
        "OnRecvPacketMsg",
        &[
            ReturnTypeExpectation::ResultVecU8,
            ReturnTypeExpectation::ResultUnit,
        ],
        "OnRecvPacket",
    ),
    CallbackConfig::new(
        "on_acknowledgement_packet",
        "OnAcknowledgementPacketMsg",
        &[ReturnTypeExpectation::ResultUnit],
        "OnAcknowledgementPacket",
    ),
    CallbackConfig::new(
        "on_timeout_packet",
        "OnTimeoutPacketMsg",
        &[ReturnTypeExpectation::ResultUnit],
        "OnTimeoutPacket",
    ),
];

// ============================================================================
// Main Macro Entry Point
// ============================================================================

/// Attribute macro for IBC applications
///
/// This macro wraps Anchor's `#[program]` macro and adds compile-time validation
/// to ensure all required IBC callback functions are implemented with correct names.
///
/// # Required Callbacks
///
/// Your IBC app MUST implement these three functions:
///
/// 1. `on_recv_packet` - Handle incoming packets from counterparty chain
/// 2. `on_acknowledgement_packet` - Handle acknowledgements for sent packets (NOT `on_ack_packet`)
/// 3. `on_timeout_packet` - Handle timeouts for sent packets
///
/// # Example
///
/// ```ignore
/// use solana_ibc_macros::ibc_app;
///
/// declare_id!("...");
///
/// #[ibc_app]
/// pub mod my_ibc_app {
///     use super::*;
///
///     pub fn on_recv_packet<'info>(
///         ctx: Context<'_, '_, '_, 'info, OnRecvPacket<'info>>,
///         msg: OnRecvPacketMsg,
///     ) -> Result<Vec<u8>> {
///         // Handle received packet
///         Ok(vec![])
///     }
///
///     pub fn on_acknowledgement_packet(
///         ctx: Context<OnAckPacket>,
///         msg: OnAcknowledgementPacketMsg,
///     ) -> Result<()> {
///         // Handle acknowledgement
///         Ok(())
///     }
///
///     pub fn on_timeout_packet(
///         ctx: Context<OnTimeoutPacket>,
///         msg: OnTimeoutPacketMsg,
///     ) -> Result<()> {
///         // Handle timeout
///         Ok(())
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn ibc_app(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let module = parse_macro_input!(item as ItemMod);

    // Validate that all required callbacks are present with correct signatures
    let validator = CallbackValidator::new(&module);
    if let Err(e) = validator.validate() {
        return e.to_compile_error().into();
    }

    // Generate output with compile-time assertions
    generate_output(module)
}

// ============================================================================
// Validation Logic
// ============================================================================

struct CallbackValidator<'a> {
    module: &'a ItemMod,
    functions: HashMap<String, &'a ItemFn>,
}

impl<'a> CallbackValidator<'a> {
    fn new(module: &'a ItemMod) -> Self {
        let functions = Self::collect_functions(module);
        Self { module, functions }
    }

    fn validate(&self) -> syn::Result<()> {
        // Ensure module has content
        self.validate_module_content()?;

        // Check for missing callbacks
        self.validate_missing_callbacks()?;

        // Validate signatures of present callbacks
        self.validate_callback_signatures()?;

        Ok(())
    }

    fn collect_functions(module: &'a ItemMod) -> HashMap<String, &'a ItemFn> {
        let mut functions = HashMap::new();

        if let Some((_, items)) = &module.content {
            for item in items {
                if let syn::Item::Fn(item_fn) = item {
                    functions.insert(item_fn.sig.ident.to_string(), item_fn);
                }
            }
        }

        functions
    }

    fn validate_module_content(&self) -> syn::Result<()> {
        if self.module.content.is_none() {
            return Err(syn::Error::new_spanned(
                self.module,
                "IBC app module must have a body with callback implementations",
            ));
        }
        Ok(())
    }

    fn validate_missing_callbacks(&self) -> syn::Result<()> {
        let missing_callbacks: Vec<_> = IBC_CALLBACKS
            .iter()
            .filter(|cb| !self.functions.contains_key(cb.name))
            .collect();

        if missing_callbacks.is_empty() {
            return Ok(());
        }

        let error_msg = self.build_missing_callbacks_error(&missing_callbacks);
        Err(syn::Error::new_spanned(self.module, error_msg))
    }

    fn build_missing_callbacks_error(&self, missing: &[&CallbackConfig]) -> String {
        let missing_names: Vec<_> = missing.iter().map(|cb| cb.name).collect();
        let mut error_msg = format!(
            "IBC app is missing required callback function(s): {}",
            missing_names.join(", ")
        );

        // Check for common naming mistakes
        if let Some(suggestion) = self.check_common_mistakes(&missing_names) {
            error_msg.push_str(&suggestion);
        }

        error_msg
    }

    fn check_common_mistakes(&self, missing: &[&str]) -> Option<String> {
        // Check for shortened acknowledgement callback name
        if missing.contains(&"on_acknowledgement_packet")
            && self.functions.contains_key("on_ack_packet")
        {
            return Some(
                "\n\nFound 'on_ack_packet' but expected 'on_acknowledgement_packet'.\n\
                 The router expects the full name 'on_acknowledgement_packet', not 'on_ack_packet'.".into()
            );
        }

        // Could add more common mistake patterns here
        None
    }

    fn validate_callback_signatures(&self) -> syn::Result<()> {
        for callback in IBC_CALLBACKS {
            if let Some(item_fn) = self.functions.get(callback.name) {
                SignatureValidator::new(callback, item_fn).validate()?;
            }
        }
        Ok(())
    }
}

// ============================================================================
// Signature Validation
// ============================================================================

struct SignatureValidator<'a> {
    config: &'a CallbackConfig,
    item_fn: &'a ItemFn,
}

impl<'a> SignatureValidator<'a> {
    const fn new(config: &'a CallbackConfig, item_fn: &'a ItemFn) -> Self {
        Self { config, item_fn }
    }

    fn validate(&self) -> syn::Result<()> {
        self.validate_parameter_count()?;
        self.validate_return_type()?;
        self.validate_message_parameter()?;
        Ok(())
    }

    fn validate_parameter_count(&self) -> syn::Result<()> {
        let sig = &self.item_fn.sig;
        if sig.inputs.len() != 2 {
            return Err(syn::Error::new_spanned(
                &sig.inputs,
                format!(
                    "Callback '{}' must have exactly 2 parameters (ctx: Context<...>, msg: {}), found {}",
                    self.config.name,
                    self.config.msg_type,
                    sig.inputs.len()
                ),
            ));
        }
        Ok(())
    }

    fn validate_return_type(&self) -> syn::Result<()> {
        let sig = &self.item_fn.sig;
        let return_type = &sig.output;

        let is_valid = self
            .config
            .return_types
            .iter()
            .any(|expected| match expected {
                ReturnTypeExpectation::ResultUnit => is_result_unit(return_type),
                ReturnTypeExpectation::ResultVecU8 => is_result_vec_u8(return_type),
            });

        if !is_valid {
            let expected_types = self.format_expected_return_types();
            return Err(syn::Error::new_spanned(
                return_type,
                format!(
                    "Callback '{}' must return {}, found '{}'",
                    self.config.name,
                    expected_types,
                    quote::quote!(#return_type)
                ),
            ));
        }

        Ok(())
    }

    fn format_expected_return_types(&self) -> String {
        let type_strings: Vec<_> = self
            .config
            .return_types
            .iter()
            .map(|rt| match rt {
                ReturnTypeExpectation::ResultUnit => "'Result<()>'",
                ReturnTypeExpectation::ResultVecU8 => "'Result<Vec<u8>>'",
            })
            .collect();

        if type_strings.len() == 1 {
            type_strings[0].to_string()
        } else {
            format!("one of: {}", type_strings.join(", "))
        }
    }

    fn validate_message_parameter(&self) -> syn::Result<()> {
        let sig = &self.item_fn.sig;

        if let Some(FnArg::Typed(pat_type)) = sig.inputs.iter().nth(1) {
            if !type_ends_with(&pat_type.ty, self.config.msg_type) {
                return Err(syn::Error::new_spanned(
                    &pat_type.ty,
                    format!(
                        "Callback '{}' second parameter must be of type '{}', found '{}'",
                        self.config.name,
                        self.config.msg_type,
                        quote::quote!(#(&pat_type.ty))
                    ),
                ));
            }
        }

        Ok(())
    }
}

// ============================================================================
// Type Checking Utilities
// ============================================================================

/// Check if return type is Result<Vec<u8>>
fn is_result_vec_u8(return_type: &ReturnType) -> bool {
    matches_result_type(return_type)
}

/// Check if return type is Result<()>
fn is_result_unit(return_type: &ReturnType) -> bool {
    matches_result_type(return_type)
}

/// Generic check if return type is a Result
fn matches_result_type(return_type: &ReturnType) -> bool {
    if let ReturnType::Type(_, ty) = return_type {
        if let Type::Path(TypePath { path, .. }) = &**ty {
            if let Some(segment) = path.segments.last() {
                return segment.ident == "Result";
            }
        }
    }
    false
}

/// Check if a type path ends with the expected identifier
fn type_ends_with(ty: &Type, expected: &str) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            return segment.ident == expected;
        }
    }
    false
}

// ============================================================================
// Code Generation
// ============================================================================

fn generate_output(module: ItemMod) -> TokenStream {
    let discriminator_checks = generate_discriminator_checks();

    let output = quote! {
        #[::anchor_lang::program]
        #module

        // Compile-time check that instruction discriminators exist with correct names
        const _: () = {
            use ::anchor_lang::Discriminator;
            #discriminator_checks
        };
    };

    TokenStream::from(output)
}

fn generate_discriminator_checks() -> proc_macro2::TokenStream {
    let checks = IBC_CALLBACKS.iter().map(|callback| {
        let discriminator_path = format!("crate::instruction::{}", callback.discriminator_name);
        let discriminator_ident: proc_macro2::TokenStream = discriminator_path.parse().unwrap();

        quote! {
            // Verify #callback.name discriminator exists
            let _ = #discriminator_ident::DISCRIMINATOR;
        }
    });

    quote! {
        #(#checks)*
    }
}

// ============================================================================
// Discriminator Macro
// ============================================================================

/// Computes an Anchor instruction discriminator at compile time.
///
/// The discriminator is the first 8 bytes of `sha256("global:<instruction_name>")`.
///
/// # Example
///
/// ```ignore
/// use solana_ibc_macros::discriminator;
///
/// const ON_RECV_PACKET_DISCRIMINATOR: [u8; 8] = discriminator!("on_recv_packet");
/// ```
#[proc_macro]
pub fn discriminator(input: TokenStream) -> TokenStream {
    let instruction_name = parse_macro_input!(input as LitStr).value();

    let preimage = format!("global:{instruction_name}");
    let hash = Sha256::digest(preimage.as_bytes());
    let bytes: [u8; 8] = hash[..8].try_into().unwrap();

    let b0 = bytes[0];
    let b1 = bytes[1];
    let b2 = bytes[2];
    let b3 = bytes[3];
    let b4 = bytes[4];
    let b5 = bytes[5];
    let b6 = bytes[6];
    let b7 = bytes[7];

    let output = quote! {
        [#b0, #b1, #b2, #b3, #b4, #b5, #b6, #b7]
    };

    output.into()
}
