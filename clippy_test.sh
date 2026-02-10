#!/usr/bin/env bash
#a lot of these lints are...naturally...extremely pedantic.

cargo clippy --all -- \
  -W clippy::all \
  -W clippy::pedantic \
  -W clippy::restriction \
  -W clippy::nursery \
  -D warnings \
  -A clippy::arithmetic_side_effects \
  -A clippy::default_numeric_fallback \
  -A clippy::as_conversions \
  -A clippy::wildcard_enum_match_arm \
  -A clippy::question_mark_used \
  -A clippy::semicolon_if_nothing_returned \
  -A clippy::missing_trait_methods \
  -A clippy::semicolon_inside_block \
  -A clippy::implicit_return \
  -A clippy::as_underscore \
  -A clippy::min_ident_chars \
  -A clippy::missing_docs_in_private_items \
  -A clippy::blanket_clippy_restriction_lints \
  -A clippy::absolute_paths \
  -A clippy::arbitrary_source_item_ordering \
  -A clippy::std_instead_of_alloc \
  -A clippy::unseparated_literal_suffix \
  -A clippy::pub_use \
  -A clippy::field_scoped_visibility_modifiers \
  -A clippy::pub_with_shorthand \
  -A clippy::allow_attributes \
  -A clippy::allow_attributes_without_reason \
  -A clippy::single_call_fn \
  -A clippy::absolute_paths \
  -A clippy::let_underscore_untyped \
  -A clippy::items_after_statements \
  -A clippy::mod_module_files
