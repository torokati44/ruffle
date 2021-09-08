//! Tests running SWFs in a headless Ruffle instance.
//!
//! Trace output can be compared with correct output from the official Flash Player.

use approx::assert_relative_eq;
use ruffle_core::backend::render::RenderBackend;
use ruffle_core::backend::video::SoftwareVideoBackend;
use ruffle_core::backend::video::VideoBackend;
use ruffle_core::backend::{
    audio::NullAudioBackend,
    locale::NullLocaleBackend,
    log::LogBackend,
    navigator::{NullExecutor, NullNavigatorBackend},
    render::NullRenderer,
    storage::{MemoryStorageBackend, StorageBackend},
    ui::NullUiBackend,
    video::NullVideoBackend,
};
use ruffle_core::context::UpdateContext;
use ruffle_core::external::Value as ExternalValue;
use ruffle_core::external::{ExternalInterfaceMethod, ExternalInterfaceProvider};
use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::Player;
use ruffle_render_wgpu::target::TextureTarget;
use ruffle_render_wgpu::wgpu;
use ruffle_render_wgpu::WgpuRenderBackend;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn get_img_platform_suffix(info: &wgpu::AdapterInfo) -> String {
    format!("{}-{}", std::env::consts::OS, info.name)
}

const RUN_IMG_TESTS: bool = cfg!(feature = "imgtests");

fn set_logger() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .is_test(true)
        .try_init();
}

type Error = Box<dyn std::error::Error>;

macro_rules! val_or_false {
    ($val:literal) => {
        $val
    };
    () => {
        false
    };
}

// This macro generates test cases for a given list of SWFs.
// If 'img' is true, then we will render an image of the final frame
// of the SWF, and compare it against a reference image on disk.
macro_rules! swf_tests {
    ($($(#[$attr:meta])* ($name:ident, $path:expr, $num_frames:literal $(, img = $img:literal)? ),)*) => {
        $(
        #[test]
        $(#[$attr])*
        fn $name() -> Result<(), Error> {
            set_logger();
            test_swf(
                concat!("tests/swfs/", $path, "/test.swf"),
                $num_frames,
                concat!("tests/swfs/", $path, "/output.txt"),
                val_or_false!($($img)?)
            )
        }
        )*
    };
}

// This macro generates test cases for a given list of SWFs using `test_swf_approx`.
macro_rules! swf_tests_approx {
    ($($(#[$attr:meta])* ($name:ident, $path:expr, $num_frames:literal $(, $opt:ident = $val:expr)*),)*) => {
        $(
        #[test]
        $(#[$attr])*
        fn $name() -> Result<(), Error> {
            set_logger();
            test_swf_approx(
                concat!("tests/swfs/", $path, "/test.swf"),
                $num_frames,
                concat!("tests/swfs/", $path, "/output.txt"),
                |actual, expected| assert_relative_eq!(actual, expected $(, $opt = $val)*),
            )
        }
        )*
    };
}

// List of SWFs to test.
// Format: (test_name, test_folder, number_of_frames_to_run)
// The test folder is a relative to core/tests/swfs
// Inside the folder is expected to be "test.swf" and "output.txt" with the correct output.
swf_tests! {
    (add_property, "avm1/add_property", 1),
    (as_transformed_flag, "avm1/as_transformed_flag", 3),
    (as_broadcaster, "avm1/as_broadcaster", 1),
    (as_broadcaster_initialize, "avm1/as_broadcaster_initialize", 1),
    (as_set_prop_flags, "avm1/as_set_prop_flags", 1),
    (attach_movie, "avm1/attach_movie", 1),
    (as2_bitor, "avm1/bitor", 1),
    (as2_bitand, "avm1/bitand", 1),
    (as2_bitxor, "avm1/bitxor", 1),
    (function_base_clip, "avm1/function_base_clip", 2),
    (call, "avm1/call", 2),
    (color, "avm1/color", 1, img = true),
    (clip_events, "avm1/clip_events", 4),
    (unload_clip_event, "avm1/unload_clip_event", 2),
    (create_empty_movie_clip, "avm1/create_empty_movie_clip", 2),
    (empty_movieclip_can_attach_movies, "avm1/empty_movieclip_can_attach_movies", 1),
    (duplicate_movie_clip, "avm1/duplicate_movie_clip", 1),
    (mouse_listeners, "avm1/mouse_listeners", 1),
    (do_init_action, "avm1/do_init_action", 3),
    (execution_order1, "avm1/execution_order1", 3),
    (execution_order2, "avm1/execution_order2", 15),
    (execution_order3, "avm1/execution_order3", 5),
    (execution_order4, "avm1/execution_order4", 4),
    (export_assets, "avm1/export_assets", 1),
    (single_frame, "avm1/single_frame", 2),
    (looping, "avm1/looping", 6),
    (matrix, "avm1/matrix", 1),
    (point, "avm1/point", 1),
    (rectangle, "avm1/rectangle", 1),
    (date_is_special, "avm1/date_is_special", 1),
    (get_bytes_total, "avm1/get_bytes_total", 1),
    (goto_advance1, "avm1/goto_advance1", 2),
    (goto_advance2, "avm1/goto_advance2", 2),
    (goto_both_ways1, "avm1/goto_both_ways1", 2),
    (goto_both_ways2, "avm1/goto_both_ways2", 3),
    (goto_frame, "avm1/goto_frame", 3),
    (goto_frame2, "avm1/goto_frame2", 5),
    (goto_frame_number, "avm1/goto_frame_number", 4),
    (goto_label, "avm1/goto_label", 4),
    (goto_methods, "avm1/goto_methods", 1),
    (goto_rewind1, "avm1/goto_rewind1", 4),
    (goto_rewind2, "avm1/goto_rewind2", 5),
    (goto_rewind3, "avm1/goto_rewind3", 2),
    (goto_execution_order, "avm1/goto_execution_order", 3),
    (goto_execution_order2, "avm1/goto_execution_order2", 2),
    (greaterthan_swf5, "avm1/greaterthan_swf5", 1),
    (greaterthan_swf8, "avm1/greaterthan_swf8", 1),
    (strictly_equals, "avm1/strictly_equals", 1),
    (tell_target, "avm1/tell_target", 3),
    (typeofs, "avm1/typeof", 1),
    (typeof_globals, "avm1/typeof_globals", 1),
    (closure_scope, "avm1/closure_scope", 1),
    (variable_args, "avm1/variable_args", 1),
    (custom_clip_methods, "avm1/custom_clip_methods", 3),
    (delete, "avm1/delete", 3),
    (selection, "avm1/selection", 1),
    (default_names, "avm1/default_names", 6),
    (array_trivial, "avm1/array_trivial", 1),
    (array_concat, "avm1/array_concat", 1),
    (array_slice, "avm1/array_slice", 1),
    (array_splice, "avm1/array_splice", 1),
    (array_properties, "avm1/array_properties", 1),
    (array_prototyping, "avm1/array_prototyping", 1),
    (array_length, "avm1/array_length", 1),
    (array_sort, "avm1/array_sort", 1),
    (array_enumerate, "avm1/array_enumerate", 1),
    (timeline_function_def, "avm1/timeline_function_def", 3),
    (root_global_parent, "avm1/root_global_parent", 3),
    (register_underflow, "avm1/register_underflow", 1),
    (object_prototypes, "avm1/object_prototypes", 1),
    (movieclip_prototype_extension, "avm1/movieclip_prototype_extension", 1),
    (movieclip_hittest, "avm1/movieclip_hittest", 1),
    (movieclip_hittest_shapeflag, "avm1/movieclip_hittest_shapeflag", 10),
    (movieclip_lockroot, "avm1/movieclip_lockroot", 10),
    #[ignore] (textfield_text, "avm1/textfield_text", 1),
    (recursive_prototypes, "avm1/recursive_prototypes", 2),
    (stage_object_children, "avm1/stage_object_children", 2),
    (has_own_property, "avm1/has_own_property", 1),
    (extends_chain, "avm1/extends_chain", 1),
    (is_prototype_of, "avm1/is_prototype_of", 1),
    #[ignore] (string_coercion, "avm1/string_coercion", 1),
    (lessthan_swf4, "avm1/lessthan_swf4", 1),
    (lessthan2_swf5, "avm1/lessthan2_swf5", 1),
    (lessthan2_swf6, "avm1/lessthan2_swf6", 1),
    (lessthan2_swf7, "avm1/lessthan2_swf7", 1),
    (logical_ops_swf4, "avm1/logical_ops_swf4", 1),
    (logical_ops_swf8, "avm1/logical_ops_swf8", 1),
    (movieclip_get_instance_at_depth, "avm1/movieclip_get_instance_at_depth", 1),
    (movieclip_depth_methods, "avm1/movieclip_depth_methods", 3),
    (get_variable_in_scope, "avm1/get_variable_in_scope", 1),
    (movieclip_init_object, "avm1/movieclip_init_object", 1),
    (greater_swf6, "avm1/greater_swf6", 1),
    (greater_swf7, "avm1/greater_swf7", 1),
    (equals_swf4, "avm1/equals_swf4", 1),
    (equals2_swf5, "avm1/equals2_swf5", 1),
    (equals2_swf6, "avm1/equals2_swf6", 1),
    (equals2_swf7, "avm1/equals2_swf7", 1),
    (escape, "avm1/escape", 1),
    (unescape, "avm1/unescape", 1),
    (register_class, "avm1/register_class", 1),
    (register_class_return_value, "avm1/register_class_return_value", 1),
    (register_class_swf6, "avm1/register_class_swf6", 1),
    (register_and_init_order, "avm1/register_and_init_order", 1),
    (on_construct, "avm1/on_construct", 1),
    (set_variable_scope, "avm1/set_variable_scope", 1),
    (slash_syntax, "avm1/slash_syntax", 2),
    (strictequals_swf6, "avm1/strictequals_swf6", 1),
    (string_methods, "avm1/string_methods", 1),
    (string_methods_negative_args, "avm1/string_methods_negative_args", 1),
    (string_ops_swf6, "avm1/string_ops_swf6", 1),
    (path_string, "avm1/path_string", 1),
    (global_is_bare, "avm1/global_is_bare", 1),
    (primitive_type_globals, "avm1/primitive_type_globals", 1),
    (primitive_instanceof, "avm1/primitive_instanceof", 1),
    (as2_oop, "avm1/as2_oop", 1),
    (extends_native_type, "avm1/extends_native_type", 1),
    (xml, "avm1/xml", 1),
    (xml_namespaces, "avm1/xml_namespaces", 1),
    (xml_node_namespaceuri, "avm1/xml_node_namespaceuri", 1),
    (xml_node_weirdnamespace, "avm1/xml_node_weirdnamespace", 1),
    (xml_clone_expandos, "avm1/xml_clone_expandos", 1),
    (xml_has_child_nodes, "avm1/xml_has_child_nodes", 1),
    (xml_first_last_child, "avm1/xml_first_last_child", 1),
    (xml_parent_and_child, "avm1/xml_parent_and_child", 1),
    (xml_siblings, "avm1/xml_siblings", 1),
    (xml_attributes_read, "avm1/xml_attributes_read", 1),
    (xml_append_child, "avm1/xml_append_child", 1),
    (xml_append_child_with_parent, "avm1/xml_append_child_with_parent", 1),
    (xml_remove_node, "avm1/xml_remove_node", 1),
    (xml_reparenting, "avm1/xml_reparenting", 1),
    (xml_insert_before, "avm1/xml_insert_before", 1),
    (xml_to_string, "avm1/xml_to_string", 1),
    (xml_to_string_comment, "avm1/xml_to_string_comment", 1),
    (xml_idmap, "avm1/xml_idmap", 1),
    (xml_ignore_comments, "avm1/xml_ignore_comments", 1),
    (xml_ignore_white, "avm1/xml_ignore_white", 1),
    (xml_inspect_doctype, "avm1/xml_inspect_doctype", 1),
    (xml_unescaping, "avm1/xml_unescaping", 1),
    #[ignore] (xml_inspect_xmldecl, "avm1/xml_inspect_xmldecl", 1),
    (xml_inspect_createmethods, "avm1/xml_inspect_createmethods", 1),
    (xml_inspect_parsexml, "avm1/xml_inspect_parsexml", 1),
    (xml_cdata, "avm1/xml_cdata", 1),
    (funky_function_calls, "avm1/funky_function_calls", 1),
    (undefined_to_string_swf6, "avm1/undefined_to_string_swf6", 1),
    (define_function_case_sensitive, "avm1/define_function_case_sensitive", 2),
    (define_function2_preload, "avm1/define_function2_preload", 1),
    (define_function2_preload_order, "avm1/define_function2_preload_order", 1),
    (mcl_as_broadcaster, "avm1/mcl_as_broadcaster", 1),
    (uncaught_exception, "avm1/uncaught_exception", 1),
    (uncaught_exception_bubbled, "avm1/uncaught_exception_bubbled", 1),
    (try_catch_finally, "avm1/try_catch_finally", 1),
    (try_finally_simple, "avm1/try_finally_simple", 1),
    (loadmovie, "avm1/loadmovie", 2),
    (loadmovienum, "avm1/loadmovienum", 2),
    (loadmovie_registerclass, "avm1/loadmovie_registerclass", 2),
    (loadmovie_replace_root, "avm1/loadmovie_replace_root", 3),
    (loadmovie_method, "avm1/loadmovie_method", 2),
    (loadmovie_fail, "avm1/loadmovie_fail", 1),
    (unloadmovie, "avm1/unloadmovie", 11),
    (unloadmovienum, "avm1/unloadmovienum", 11),
    (unloadmovie_method, "avm1/unloadmovie_method", 11),
    (mcl_loadclip, "avm1/mcl_loadclip", 11),
    (mcl_unloadclip, "avm1/mcl_unloadclip", 11),
    (mcl_getprogress, "avm1/mcl_getprogress", 6),
    (load_vars, "avm1/load_vars", 2),
    (loadvariables, "avm1/loadvariables", 3),
    (loadvariablesnum, "avm1/loadvariablesnum", 3),
    (loadvariables_method, "avm1/loadvariables_method", 3),
    (xml_load, "avm1/xml_load", 1),
    (with_return, "avm1/with_return", 1),
    (watch, "avm1/watch", 1),
    #[ignore] (watch_virtual_property, "avm1/watch_virtual_property", 1),
    (cross_movie_root, "avm1/cross_movie_root", 5),
    (roots_and_levels, "avm1/roots_and_levels", 1),
    (swf5_encoding, "avm1/swf5_encoding", 1),
    (swf6_case_insensitive, "avm1/swf6_case_insensitive", 1),
    (swf7_case_sensitive, "avm1/swf7_case_sensitive", 1),
    (prototype_enumerate, "avm1/prototype_enumerate", 1),
    (stage_object_enumerate, "avm1/stage_object_enumerate", 1),
    (new_object_enumerate, "avm1/new_object_enumerate", 1),
    (as2_super_and_this_v6, "avm1/as2_super_and_this_v6", 1),
    (as2_super_and_this_v8, "avm1/as2_super_and_this_v8", 1),
    (as2_super_via_manual_prototype, "avm1/as2_super_via_manual_prototype", 1),
    (as1_constructor_v6, "avm1/as1_constructor_v6", 1),
    (as1_constructor_v7, "avm1/as1_constructor_v7", 1),
    (issue_710, "avm1/issue_710", 1),
    (issue_1086, "avm1/issue_1086", 1),
    (issue_1104, "avm1/issue_1104", 3),
    (issue_1671, "avm1/issue_1671", 1),
    (issue_1906, "avm1/issue_1906", 2),
    (issue_2030, "avm1/issue_2030", 1),
    (issue_2084, "avm1/issue_2084", 2),
    (issue_2166, "avm1/issue_2166", 1),
    (issue_2870, "avm1/issue_2870", 10),
    (issue_3169, "avm1/issue_3169", 1),
    (issue_3446, "avm1/issue_3446", 1),
    (issue_3522, "avm1/issue_3522", 2),
    (issue_4377, "avm1/issue_4377", 1),
    (function_as_function, "avm1/function_as_function", 1),
    (infinite_recursion_function, "avm1/infinite_recursion_function", 1),
    (infinite_recursion_function_in_setter, "avm1/infinite_recursion_function_in_setter", 1),
    (infinite_recursion_virtual_property, "avm1/infinite_recursion_virtual_property", 1),
    (edittext_font_size, "avm1/edittext_font_size", 1),
    (edittext_default_format, "avm1/edittext_default_format", 1),
    (edittext_leading, "avm1/edittext_leading", 1),
    #[ignore] (edittext_newlines, "avm1/edittext_newlines", 1),
    (edittext_html_entity, "avm1/edittext_html_entity", 1),
    (edittext_password, "avm1/edittext_password", 1),
    (edittext_scroll, "avm1/edittext_scroll", 1),
    #[ignore] (edittext_html_roundtrip, "avm1/edittext_html_roundtrip", 1),
    (edittext_newline_stripping, "avm1/edittext_newline_stripping", 1),
    (define_local, "avm1/define_local", 1),
    (textfield_properties, "avm1/textfield_properties", 1),
    (textfield_background_color, "avm1/textfield_background_color", 1),
    (textfield_border_color, "avm1/textfield_border_color", 1),
    (textfield_variable, "avm1/textfield_variable", 8),
    (error, "avm1/error", 1),
    (color_transform, "avm1/color_transform", 1),
    (with, "avm1/with", 1),
    (arguments, "avm1/arguments", 1),
    (prototype_properties, "avm1/prototype_properties", 1),
    (stage_object_properties_get_var, "avm1/stage_object_properties_get_var", 1),
    (set_interval, "avm1/set_interval", 20),
    (context_menu, "avm1/context_menu", 1),
    (context_menu_item, "avm1/context_menu_item", 1),
    (constructor_function, "avm1/constructor_function", 1),
    (global_array, "avm1/global_array", 1),
    (array_constructor, "avm1/array_constructor", 1),
    (object_constructor, "avm1/object_constructor", 1),
    (object_function, "avm1/object_function", 1),
    (parse_int, "avm1/parse_int", 1),
    (bitmap_filter, "avm1/bitmap_filter", 1),
    (blur_filter, "avm1/blur_filter", 1),
    (glow_filter, "avm1/glow_filter", 1),
    (date_constructor, "avm1/date/constructor", 1),
    (removed_clip_halts_script, "avm1/removed_clip_halts_script", 13),
    (target_clip_removed, "avm1/target_clip_removed", 1),
    (date_utc, "avm1/date/UTC", 1),
    (date_set_date, "avm1/date/setDate", 1),
    (date_set_full_year, "avm1/date/setFullYear", 1),
    (date_set_hours, "avm1/date/setHours", 1),
    (date_set_milliseconds, "avm1/date/setMilliseconds", 1),
    (date_set_minutes, "avm1/date/setMinutes", 1),
    (date_set_month, "avm1/date/setMonth", 1),
    (date_set_seconds, "avm1/date/setSeconds", 1),
    (date_set_time, "avm1/date/setTime", 1),
    (date_set_utc_date, "avm1/date/setUTCDate", 1),
    (date_set_utc_full_year, "avm1/date/setUTCFullYear", 1),
    (date_set_utc_hours, "avm1/date/setUTCHours", 1),
    (date_set_utc_milliseconds, "avm1/date/setUTCMilliseconds", 1),
    (date_set_utc_minutes, "avm1/date/setUTCMinutes", 1),
    (date_set_utc_month, "avm1/date/setUTCMonth", 1),
    (date_set_utc_seconds, "avm1/date/setUTCSeconds", 1),
    (date_set_year, "avm1/date/setYear", 1),
    (this_scoping, "avm1/this_scoping", 1),
    (bevel_filter, "avm1/bevel_filter", 1),
    (drop_shadow_filter, "avm1/drop_shadow_filter", 1),
    (color_matrix_filter, "avm1/color_matrix_filter", 1),
    (displacement_map_filter, "avm1/displacement_map_filter", 1),
    (convolution_filter, "avm1/convolution_filter", 1),
    (gradient_bevel_filter, "avm1/gradient_bevel_filter", 1),
    (gradient_glow_filter, "avm1/gradient_glow_filter", 1),
    (bitmap_data, "avm1/bitmap_data", 1),
    (bitmap_data_max_size_swf9, "avm1/bitmap_data_max_size_swf9", 1),
    (bitmap_data_max_size_swf10, "avm1/bitmap_data_max_size_swf10", 1),
    (bitmap_data_noise, "avm1/bitmap_data_noise", 1),
    (array_call_method, "avm1/array_call_method", 1),
    (bad_placeobject_clipaction, "avm1/bad_placeobject_clipaction", 2),
    (bad_swf_tag_past_eof, "avm1/bad_swf_tag_past_eof", 1),
    (sound, "avm1/sound", 1),
    (action_to_integer, "avm1/action_to_integer", 1),
    (call_method_empty_name, "avm1/call_method_empty_name", 1),
    (init_array_invalid, "avm1/init_array_invalid", 1),
    (init_object_invalid, "avm1/init_array_invalid", 1),
    (new_object_wrap, "avm1/new_object_wrap", 1),
    (new_method_wrap, "avm1/new_method_wrap", 1),
    (as3_hello_world, "avm2/hello_world", 1),
    (as3_function_call, "avm2/function_call", 1),
    (as3_function_call_via_call, "avm2/function_call_via_call", 1),
    (as3_constructor_call, "avm2/constructor_call", 1),
    (as3_class_methods, "avm2/class_methods", 1),
    (as3_es3_inheritance, "avm2/es3_inheritance", 1),
    (as3_es4_inheritance, "avm2/es4_inheritance", 1),
    (as3_stored_properties, "avm2/stored_properties", 1),
    (as3_virtual_properties, "avm2/virtual_properties", 1),
    (as3_es4_oop_prototypes, "avm2/es4_oop_prototypes", 1),
    (as3_es4_method_binding, "avm2/es4_method_binding", 1),
    (as3_control_flow_bool, "avm2/control_flow_bool", 1),
    (as3_control_flow_stricteq, "avm2/control_flow_stricteq", 1),
    (as3_object_enumeration, "avm2/object_enumeration", 1),
    (as3_object_prototype, "avm2/object_prototype", 1),
    (as3_class_enumeration, "avm2/class_enumeration", 1),
    (as3_is_prototype_of, "avm2/is_prototype_of", 1),
    (as3_has_own_property, "avm2/has_own_property", 1),
    (as3_property_is_enumerable, "avm2/property_is_enumerable", 1),
    (as3_set_property_is_enumerable, "avm2/set_property_is_enumerable", 1),
    (as3_object_to_string, "avm2/object_to_string", 1),
    (as3_function_to_string, "avm2/function_to_string", 1),
    (as3_class_to_string, "avm2/class_to_string", 1),
    (as3_object_to_locale_string, "avm2/object_to_locale_string", 1),
    (as3_function_to_locale_string, "avm2/function_to_locale_string", 1),
    (as3_class_to_locale_string, "avm2/class_to_locale_string", 1),
    (as3_object_value_of, "avm2/object_value_of", 1),
    (as3_function_value_of, "avm2/function_value_of", 1),
    (as3_class_value_of, "avm2/class_value_of", 1),
    (as3_if_stricteq, "avm2/if_stricteq", 1),
    (as3_if_strictne, "avm2/if_strictne", 1),
    (as3_strict_equality, "avm2/strict_equality", 1),
    (as3_es4_interfaces, "avm2/es4_interfaces", 1),
    (as3_is_finite, "avm2/is_finite", 1),
    (as3_is_nan, "avm2/is_nan", 1),
    (as3_istype, "avm2/istype", 1),
    (as3_istypelate, "avm2/istypelate", 1),
    (as3_instanceof, "avm2/instanceof", 1),
    (as3_astype, "avm2/astype", 1),
    (as3_astypelate, "avm2/astypelate", 1),
    (as3_truthiness, "avm2/truthiness", 1),
    (as3_falsiness, "avm2/falsiness", 1),
    (as3_boolean_negation, "avm2/boolean_negation", 1),
    (as3_convert_boolean, "avm2/convert_boolean", 1),
    (as3_convert_number, "avm2/convert_number", 1),
    (as3_convert_integer, "avm2/convert_integer", 1),
    (as3_convert_uinteger, "avm2/convert_uinteger", 1),
    (as3_coerce_string, "avm2/coerce_string", 1),
    (as3_if_eq, "avm2/if_eq", 1),
    (as3_if_ne, "avm2/if_ne", 1),
    (as3_equals, "avm2/equals", 1),
    (as3_if_lt, "avm2/if_lt", 1),
    (as3_if_lte, "avm2/if_lte", 1),
    (as3_if_gte, "avm2/if_gte", 1),
    (as3_if_gt, "avm2/if_gt", 1),
    (as3_greaterequals, "avm2/greaterequals", 1),
    (as3_greaterthan, "avm2/greaterthan", 1),
    (as3_lessequals, "avm2/lessequals", 1),
    (as3_lessthan, "avm2/lessthan", 1),
    (nested_textfields_in_buttons, "avm1/nested_textfields_in_buttons", 1),
    (conflicting_instance_names, "avm1/conflicting_instance_names", 6),
    (button_children, "avm1/button_children", 1),
    (transform, "avm1/transform", 1),
    (target_clip_swf5, "avm1/target_clip_swf5", 2),
    (target_clip_swf6, "avm1/target_clip_swf6", 2),
    (target_path, "avm1/target_path", 1),
    (remove_movie_clip, "avm1/remove_movie_clip", 2),
    (as3_add, "avm2/add", 1),
    (as3_bitor, "avm2/bitor", 1),
    (as3_bitand, "avm2/bitand", 1),
    (as3_bitnot, "avm2/bitnot", 1),
    (as3_bitxor, "avm2/bitxor", 1),
    (as3_declocal, "avm2/declocal", 1),
    (as3_declocal_i, "avm2/declocal_i", 1),
    (as3_decrement, "avm2/decrement", 1),
    (as3_decrement_i, "avm2/decrement_i", 1),
    (as3_inclocal, "avm2/inclocal", 1),
    (as3_inclocal_i, "avm2/inclocal_i", 1),
    (as3_increment, "avm2/increment", 1),
    (as3_increment_i, "avm2/increment_i", 1),
    (as3_lshift, "avm2/lshift", 1),
    (as3_modulo, "avm2/modulo", 1),
    (as3_multiply, "avm2/multiply", 1),
    (as3_negate, "avm2/negate", 1),
    (as3_rshift, "avm2/rshift", 1),
    (as3_subtract, "avm2/subtract", 1),
    (as3_urshift, "avm2/urshift", 1),
    (as3_in, "avm2/in", 1),
    (as3_bytearray, "avm2/bytearray", 1),
    (as3_generate_random_bytes, "avm2/generate_random_bytes", 1),
    (as3_get_definition_by_name, "avm2/get_definition_by_name", 1),
    (as3_get_qualified_class_name, "avm2/get_qualified_class_name", 1),
    (as3_get_qualified_super_class_name, "avm2/get_qualified_super_class_name", 1),
    (as3_array_constr, "avm2/array_constr", 1),
    (as3_array_access, "avm2/array_access", 1),
    (as3_array_storage, "avm2/array_storage", 1),
    (as3_array_delete, "avm2/array_delete", 1),
    (as3_array_holes, "avm2/array_holes", 1),
    (as3_array_literal, "avm2/array_literal", 1),
    (as3_array_concat, "avm2/array_concat", 1),
    (as3_array_tostring, "avm2/array_tostring", 1),
    (as3_array_tolocalestring, "avm2/array_tolocalestring", 1),
    (as3_array_valueof, "avm2/array_valueof", 1),
    (as3_array_join, "avm2/array_join", 1),
    (as3_array_foreach, "avm2/array_foreach", 1),
    (as3_array_map, "avm2/array_map", 1),
    (as3_array_filter, "avm2/array_filter", 1),
    (as3_array_every, "avm2/array_every", 1),
    (as3_array_some, "avm2/array_some", 1),
    (as3_array_indexof, "avm2/array_indexof", 1),
    (as3_array_lastindexof, "avm2/array_lastindexof", 1),
    (as3_array_push, "avm2/array_push", 1),
    (as3_array_pop, "avm2/array_pop", 1),
    (as3_array_reverse, "avm2/array_reverse", 1),
    (as3_array_shift, "avm2/array_shift", 1),
    (as3_array_unshift, "avm2/array_unshift", 1),
    (as3_array_slice, "avm2/array_slice", 1),
    (as3_array_splice, "avm2/array_splice", 1),
    (as3_array_sort, "avm2/array_sort", 1),
    (as3_array_sorton, "avm2/array_sorton", 1),
    (as3_array_hasownproperty, "avm2/array_hasownproperty", 1),
    (as3_array_length, "avm2/array_length", 1),
    (stage_property_representation, "avm1/stage_property_representation", 1),
    (as3_timeline_scripts, "avm2/timeline_scripts", 3),
    (as3_movieclip_properties, "avm2/movieclip_properties", 4),
    (as3_movieclip_gotoandplay, "avm2/movieclip_gotoandplay", 5),
    (as3_movieclip_gotoandstop, "avm2/movieclip_gotoandstop", 5),
    (as3_movieclip_stop, "avm2/movieclip_stop", 5),
    (as3_movieclip_prev_frame, "avm2/movieclip_prev_frame", 5),
    (as3_movieclip_next_frame, "avm2/movieclip_next_frame", 5),
    (as3_movieclip_prev_scene, "avm2/movieclip_prev_scene", 5),
    (as3_movieclip_next_scene, "avm2/movieclip_next_scene", 5),
    (as3_framelabel_constr, "avm2/framelabel_constr", 5),
    (as3_movieclip_currentlabels, "avm2/movieclip_currentlabels", 5),
    (as3_scene_constr, "avm2/scene_constr", 5),
    (as3_movieclip_currentscene, "avm2/movieclip_currentscene", 5),
    (as3_movieclip_scenes, "avm2/movieclip_scenes", 5),
    (as3_movieclip_play, "avm2/movieclip_play", 5),
    (as3_movieclip_constr, "avm2/movieclip_constr", 1),
    (as3_lazyinit, "avm2/lazyinit", 1),
    (as3_trace, "avm2/trace", 1),
    (as3_displayobjectcontainer_getchildat, "avm2/displayobjectcontainer_getchildat", 1),
    (as3_displayobjectcontainer_getchildbyname, "avm2/displayobjectcontainer_getchildbyname", 1),
    (as3_displayobjectcontainer_addchild, "avm2/displayobjectcontainer_addchild", 1),
    (as3_displayobjectcontainer_addchildat, "avm2/displayobjectcontainer_addchildat", 1),
    (as3_displayobjectcontainer_removechild, "avm2/displayobjectcontainer_removechild", 1),
    (as3_displayobjectcontainer_removechild_timelinemanip_remove1, "avm2/displayobjectcontainer_removechild_timelinemanip_remove1", 7),
    (as3_displayobjectcontainer_addchild_timelinepull0, "avm2/displayobjectcontainer_addchild_timelinepull0", 7),
    (as3_displayobjectcontainer_addchild_timelinepull1, "avm2/displayobjectcontainer_addchild_timelinepull1", 7),
    (as3_displayobjectcontainer_addchild_timelinepull2, "avm2/displayobjectcontainer_addchild_timelinepull2", 7),
    (as3_displayobjectcontainer_addchildat_timelinelock0, "avm2/displayobjectcontainer_addchildat_timelinelock0", 7),
    (as3_displayobjectcontainer_addchildat_timelinelock1, "avm2/displayobjectcontainer_addchildat_timelinelock1", 7),
    (as3_displayobjectcontainer_addchildat_timelinelock2, "avm2/displayobjectcontainer_addchildat_timelinelock2", 7),
    (as3_displayobjectcontainer_contains, "avm2/displayobjectcontainer_contains", 5),
    (as3_displayobjectcontainer_getchildindex, "avm2/displayobjectcontainer_getchildindex", 5),
    (as3_displayobjectcontainer_removechildat, "avm2/displayobjectcontainer_removechildat", 1),
    (as3_displayobjectcontainer_removechildren, "avm2/displayobjectcontainer_removechildren", 5),
    (as3_displayobjectcontainer_setchildindex, "avm2/displayobjectcontainer_setchildindex", 1),
    (as3_displayobjectcontainer_swapchildren, "avm2/displayobjectcontainer_swapchildren", 1),
    (as3_displayobjectcontainer_swapchildrenat, "avm2/displayobjectcontainer_swapchildrenat", 1),
    (button_order, "avm1/button_order", 2),
    (as3_displayobjectcontainer_stopallmovieclips, "avm2/displayobjectcontainer_stopallmovieclips", 2),
    (as3_displayobjectcontainer_timelineinstance, "avm2/displayobjectcontainer_timelineinstance", 6),
    (as3_displayobject_alpha, "avm2/displayobject_alpha", 1),
    (as3_displayobject_x, "avm2/displayobject_x", 1),
    (as3_displayobject_y, "avm2/displayobject_y", 1),
    (as3_displayobject_name, "avm2/displayobject_name", 4),
    (as3_displayobject_parent, "avm2/displayobject_parent", 4),
    (as3_displayobject_root, "avm2/displayobject_root", 4),
    (as3_displayobject_visible, "avm2/displayobject_visible", 4),
    (as3_displayobject_hittestpoint, "avm2/displayobject_hittestpoint", 2),
    (as3_displayobject_hittestobject, "avm2/displayobject_hittestobject", 1),
    (as3_event_valueof_tostring, "avm2/event_valueof_tostring", 1),
    (as3_event_bubbles, "avm2/event_bubbles", 1),
    (as3_event_cancelable, "avm2/event_cancelable", 1),
    (as3_event_type, "avm2/event_type", 1),
    (as3_event_clone, "avm2/event_clone", 1),
    (as3_event_formattostring, "avm2/event_formattostring", 1),
    (as3_event_isdefaultprevented, "avm2/event_isdefaultprevented", 1),
    (as3_function_call_via_apply, "avm2/function_call_via_apply", 1),
    (as3_function_call_arguments, "avm2/function_call_arguments", 1),
    (as3_function_call_rest, "avm2/function_call_rest", 1),
    (as3_eventdispatcher_haseventlistener, "avm2/eventdispatcher_haseventlistener", 1),
    (as3_eventdispatcher_willtrigger, "avm2/eventdispatcher_willtrigger", 1),
    (as3_movieclip_willtrigger, "avm2/movieclip_willtrigger", 3),
    (as3_eventdispatcher_dispatchevent, "avm2/eventdispatcher_dispatchevent", 1),
    (as3_eventdispatcher_dispatchevent_handlerorder, "avm2/eventdispatcher_dispatchevent_handlerorder", 1),
    (as3_eventdispatcher_dispatchevent_cancel, "avm2/eventdispatcher_dispatchevent_cancel", 1),
    (as3_eventdispatcher_dispatchevent_this, "avm2/eventdispatcher_dispatchevent_this", 1),
    (as3_movieclip_dispatchevent, "avm2/movieclip_dispatchevent", 1),
    (as3_movieclip_dispatchevent_handlerorder, "avm2/movieclip_dispatchevent_handlerorder", 1),
    (as3_movieclip_dispatchevent_cancel, "avm2/movieclip_dispatchevent_cancel", 1),
    (as3_movieclip_dispatchevent_target, "avm2/movieclip_dispatchevent_target", 1),
    (as3_movieclip_dispatchevent_selfadd, "avm2/movieclip_dispatchevent_selfadd", 1),
    (as3_string_constr, "avm2/string_constr", 1),
    (as3_string_length, "avm2/string_length", 1),
    (as3_string_char_at, "avm2/string_char_at", 1),
    (as3_string_char_code_at, "avm2/string_char_code_at", 1),
    (as3_string_split, "avm2/string_split", 1),
    (as3_typeof, "avm2/typeof", 1),
    (use_hand_cursor, "avm1/use_hand_cursor", 1),
    (as3_movieclip_displayevents, "avm2/movieclip_displayevents", 9),
    (as3_movieclip_displayevents_timeline, "avm2/movieclip_displayevents_timeline", 5),
    (as3_movieclip_displayevents_looping, "avm2/movieclip_displayevents_looping", 5),
    (as3_movieclip_displayevents_dblhandler, "avm2/movieclip_displayevents_dblhandler", 4),
    (as3_regexp_constr, "avm2/regexp_constr", 1),
    (as3_regexp_test, "avm2/regexp_test", 1),
    (as3_regexp_exec, "avm2/regexp_exec", 1),
    (as3_point, "avm2/point", 1),
    (as3_edittext_default_format, "avm2/edittext_default_format", 1),
    (as3_edittext_html_entity, "avm2/edittext_html_entity", 1),
    #[ignore] (as3_edittext_html_roundtrip, "avm2/edittext_html_roundtrip", 1),
    (as3_edittext_newline_stripping, "avm2/edittext_newline_stripping", 1),
    (as3_shape_drawrect, "avm2/shape_drawrect", 1),
    (as3_movieclip_drawrect, "avm2/movieclip_drawrect", 1),
    (as3_get_timer, "avm2/get_timer", 1),
    (as3_op_escxattr, "avm2/op_escxattr", 1),
    (as3_op_escxelem, "avm2/op_escxelem", 1),
    (as3_op_lookupswitch, "avm2/op_lookupswitch", 1),
    (as3_loaderinfo_properties, "avm2/loaderinfo_properties", 2),
    (as3_loaderinfo_quine, "avm2/loaderinfo_quine", 2),
    (nan_scale, "avm1/nan_scale", 1),
    (as3_nan_scale, "avm2/nan_scale", 1),
    (as3_documentclass, "avm2/documentclass", 1),
    (timer_run_actions, "avm1/timer_run_actions", 1),
    (as3_op_coerce, "avm2/op_coerce", 1),
    (as3_domain_memory, "avm2/domain_memory", 1),
    (as3_movieclip_symbol_constr, "avm2/movieclip_symbol_constr", 1),
    (as3_stage_access, "avm2/stage_access", 1),
    (as3_stage_displayobject_properties, "avm2/stage_displayobject_properties", 1),
    (as3_stage_loaderinfo_properties, "avm2/stage_loaderinfo_properties", 2),
    (as3_stage_properties, "avm2/stage_properties", 1),
    (as3_closures, "avm2/closures", 1),
    (as3_simplebutton_structure, "avm2/simplebutton_structure", 2),
    (as3_simplebutton_childevents, "avm2/simplebutton_childevents", 2),
    (as3_simplebutton_childevents_nested, "avm2/simplebutton_childevents_nested", 2),
    (as3_simplebutton_constr, "avm2/simplebutton_constr", 2),
    (as3_simplebutton_constr_childevents, "avm2/simplebutton_constr_childevents", 2),
    (as3_simplebutton_childprops, "avm2/simplebutton_childprops", 1),
    (as3_simplebutton_childshuffle, "avm2/simplebutton_childshuffle", 1),
    (as3_simplebutton_constr_params, "avm2/simplebutton_constr_params", 1),
    (as3_place_object_replace, "avm2/place_object_replace", 2),
    (as3_place_object_replace_2, "avm2/place_object_replace_2", 3),
    (as3_function_call_default, "avm2/function_call_default", 1),
    (as3_function_call_types, "avm2/function_call_types", 1),
    (as3_function_call_coercion, "avm2/function_call_coercion", 1),
    (as3_istypelate_coerce, "avm2/istypelate_coerce", 1),
    (as3_class_cast_call, "avm2/class_cast_call", 1),
    (as3_class_supercalls_mismatched, "avm2/class_supercalls_mismatched", 1),
    (as3_symbol_class_binary_data, "avm2/symbol_class_binary_data", 1),
    (as3_rectangle, "avm2/rectangle", 1),
    (as3_font_embedded, "avm2/font_embedded", 1),
    (as3_font_hasglyphs, "avm2/font_hasglyphs", 1),
    (as3_simplebutton_symbolclass, "avm2/simplebutton_symbolclass", 3),
    (as3_vector_int_access, "avm2/vector_int_access", 1),
    (as3_vector_int_delete, "avm2/vector_int_delete", 1),
    (as3_vector_holes, "avm2/vector_holes", 1),
    (as3_vector_coercion, "avm2/vector_coercion", 1),
    (as3_vector_concat, "avm2/vector_concat", 1),
    (as3_vector_join, "avm2/vector_join", 1),
    (as3_vector_every, "avm2/vector_every", 1),
    (as3_vector_filter, "avm2/vector_filter", 1),
    (as3_vector_indexof, "avm2/vector_indexof", 1),
    (as3_vector_lastindexof, "avm2/vector_lastindexof", 1),
    (as3_vector_map, "avm2/vector_map", 1),
    (as3_vector_pushpop, "avm2/vector_pushpop", 1),
    (as3_vector_shiftunshift, "avm2/vector_shiftunshift", 1),
    (as3_vector_insertat, "avm2/vector_insertat", 1),
    (as3_vector_removeat, "avm2/vector_removeat", 1),
    (as3_vector_reverse, "avm2/vector_reverse", 1),
    (as3_vector_slice, "avm2/vector_slice", 1),
    (as3_vector_sort, "avm2/vector_sort", 1),
    (as3_vector_splice, "avm2/vector_splice", 1),
    (as3_vector_tostring, "avm2/vector_tostring", 1),
    (as3_vector_constr, "avm2/vector_constr", 1),
    (as3_vector_legacy, "avm2/vector_legacy", 1),
    (as3_sound_valueof, "avm2/sound_valueof", 1),
    (as3_sound_embeddedprops, "avm2/sound_embeddedprops", 1),
    (as3_soundtransform, "avm2/soundtransform", 1),
    (as3_movieclip_soundtransform, "avm2/movieclip_soundtransform", 49),
    (as3_simplebutton_soundtransform, "avm2/simplebutton_soundtransform", 49),
    (as3_soundmixer_soundtransform, "avm2/soundmixer_soundtransform", 49),
    (as3_sound_play, "avm2/sound_play", 1),
    #[ignore] (as3_soundchannel_position, "avm2/soundchannel_position", 75),
    (as3_soundchannel_soundtransform, "avm2/soundchannel_soundtransform", 49),
    (as3_soundchannel_stop, "avm2/soundchannel_stop", 4),
    (as3_soundmixer_stopall, "avm2/soundmixer_stopall", 4),
    #[ignore] (as3_soundchannel_soundcomplete, "avm2/soundchannel_soundcomplete", 25),
    (as3_soundmixer_buffertime, "avm2/soundmixer_buffertime", 1),
}

// TODO: These tests have some inaccuracies currently, so we use approx_eq to test that numeric values are close enough.
// Eventually we can hopefully make some of these match exactly (see #193).
// Some will probably always need to be approx. (if they rely on trig functions, etc.)
swf_tests_approx! {
    (local_to_global, "avm1/local_to_global", 1, epsilon = 0.051),
    (stage_object_properties, "avm1/stage_object_properties", 6, epsilon = 0.051),
    (stage_object_properties_swf6, "avm1/stage_object_properties_swf6", 4, epsilon = 0.051),
    (movieclip_getbounds, "avm1/movieclip_getbounds", 1, epsilon = 0.051),
    (parse_float, "avm1/parse_float", 1, max_relative = 5.0 * f64::EPSILON),
    (edittext_letter_spacing, "avm1/edittext_letter_spacing", 1, epsilon = 15.0), // TODO: Discrepancy in wrapping in letterSpacing = 0.1 test.
    (edittext_align, "avm1/edittext_align", 1, epsilon = 3.0),
    (edittext_autosize, "avm1/edittext_autosize", 1, epsilon = 4.0), // TODO Flash has _width higher by 4.0, probably padding logic mistake
    (edittext_margins, "avm1/edittext_margins", 1, epsilon = 5.0), // TODO: Discrepancy in wrapping.
    (edittext_tab_stops, "avm1/edittext_tab_stops", 1, epsilon = 5.0),
    (edittext_bullet, "avm1/edittext_bullet", 1, epsilon = 3.0),
    (edittext_underline, "avm1/edittext_underline", 1, epsilon = 4.0),
    (edittext_hscroll, "avm1/edittext_hscroll", 1, epsilon = 3.0),
    (as3_coerce_string_precision, "avm2/coerce_string_precision", 1, max_relative = 30.0 * f64::EPSILON),
    (as3_divide, "avm2/divide", 1, epsilon = 0.0), // TODO: Discrepancy in float formatting.
    (as3_math, "avm2/math", 1, max_relative = 30.0 * f64::EPSILON),
    (as3_displayobject_height, "avm2/displayobject_height", 7, epsilon = 0.06), // TODO: height/width appears to be off by 1 twip sometimes
    (as3_displayobject_width, "avm2/displayobject_width", 7, epsilon = 0.06),
    (as3_displayobject_rotation, "avm2/displayobject_rotation", 1, epsilon = 0.0000000001),
    (as3_edittext_align, "avm2/edittext_align", 1, epsilon = 3.0),
    (as3_edittext_autosize, "avm2/edittext_autosize", 1, epsilon = 5.0), // TODO AS3 has _width higher by 5.0, probably padding logic mistake
    (as3_edittext_bullet, "avm2/edittext_bullet", 1, epsilon = 3.0),
    (as3_edittext_letter_spacing, "avm2/edittext_letter_spacing", 1, epsilon = 15.0), // TODO: Discrepancy in wrapping in letterSpacing = 0.1 test.
    (as3_edittext_margins, "avm2/edittext_margins", 1, epsilon = 5.0), // TODO: Discrepancy in wrapping.
    (as3_edittext_tab_stops, "avm2/edittext_tab_stops", 1, epsilon = 5.0),
    (as3_edittext_underline, "avm2/edittext_underline", 1, epsilon = 4.0),
    (as3_edittext_leading, "avm2/edittext_leading", 1, epsilon = 0.3),
    (as3_edittext_font_size, "avm2/edittext_font_size", 1, epsilon = 0.1),
}

#[test]
fn external_interface_avm1() -> Result<(), Error> {
    set_logger();
    test_swf_with_hooks(
        "tests/swfs/avm1/external_interface/test.swf",
        1,
        "tests/swfs/avm1/external_interface/output.txt",
        |player| {
            player
                .lock()
                .unwrap()
                .add_external_interface(Box::new(ExternalInterfaceTestProvider::new()));
            Ok(())
        },
        |player| {
            let mut player_locked = player.lock().unwrap();

            let parroted =
                player_locked.call_internal_interface("parrot", vec!["Hello World!".into()]);
            player_locked.log_backend().avm_trace(&format!(
                "After calling `parrot` with a string: {:?}",
                parroted
            ));

            let mut nested = BTreeMap::new();
            nested.insert(
                "list".to_string(),
                vec![
                    "string".into(),
                    100.into(),
                    false.into(),
                    ExternalValue::Object(BTreeMap::new()),
                ]
                .into(),
            );

            let mut root = BTreeMap::new();
            root.insert("number".to_string(), (-500.1).into());
            root.insert("string".to_string(), "A string!".into());
            root.insert("true".to_string(), true.into());
            root.insert("false".to_string(), false.into());
            root.insert("null".to_string(), ExternalValue::Null);
            root.insert("nested".to_string(), nested.into());
            let result = player_locked
                .call_internal_interface("callWith", vec!["trace".into(), root.into()]);
            player_locked.log_backend().avm_trace(&format!(
                "After calling `callWith` with a complex payload: {:?}",
                result
            ));
            Ok(())
        },
        false,
    )
}

#[test]
fn shared_object_avm1() -> Result<(), Error> {
    set_logger();
    // Test SharedObject persistence. Run an SWF that saves data
    // to a shared object twice and verify that the data is saved.
    let mut memory_storage_backend: Box<dyn StorageBackend> =
        Box::new(MemoryStorageBackend::default());

    // Initial run; no shared object data.
    test_swf_with_hooks(
        "tests/swfs/avm1/shared_object/test.swf",
        1,
        "tests/swfs/avm1/shared_object/output1.txt",
        |_player| Ok(()),
        |player| {
            // Save the storage backend for next run.
            let mut player = player.lock().unwrap();
            std::mem::swap(player.storage_mut(), &mut memory_storage_backend);
            Ok(())
        },
        false,
    )?;

    // Verify that the flash cookie matches the expected one
    let expected = std::fs::read("tests/swfs/avm1/shared_object/RuffleTest.sol")?;
    assert_eq!(
        expected,
        memory_storage_backend
            .get("localhost//RuffleTest")
            .unwrap_or_default()
    );

    // Re-run the SWF, verifying that the shared object persists.
    test_swf_with_hooks(
        "tests/swfs/avm1/shared_object/test.swf",
        1,
        "tests/swfs/avm1/shared_object/output2.txt",
        |player| {
            // Swap in the previous storage backend.
            let mut player = player.lock().unwrap();
            std::mem::swap(player.storage_mut(), &mut memory_storage_backend);
            Ok(())
        },
        |_player| Ok(()),
        false,
    )?;

    Ok(())
}

#[test]
fn timeout_avm1() -> Result<(), Error> {
    set_logger();
    test_swf_with_hooks(
        "tests/swfs/avm1/timeout/test.swf",
        1,
        "tests/swfs/avm1/timeout/output.txt",
        |player| {
            player
                .lock()
                .unwrap()
                .set_max_execution_duration(Duration::from_secs(5));
            Ok(())
        },
        |_| Ok(()),
        false,
    )
}

#[test]
fn stage_scale_mode() -> Result<(), Error> {
    set_logger();
    test_swf_with_hooks(
        "tests/swfs/avm1/stage_scale_mode/test.swf",
        1,
        "tests/swfs/avm1/stage_scale_mode/output.txt",
        |player| {
            // Simulate a large viewport to test stage size.
            player
                .lock()
                .unwrap()
                .set_viewport_dimensions(900, 900, 1.0);
            Ok(())
        },
        |_| Ok(()),
        false,
    )
}

/// Wrapper around string slice that makes debug output `{:?}` to print string same way as `{}`.
/// Used in different `assert*!` macros in combination with `pretty_assertions` crate to make
/// test failures to show nice diffs.
/// Courtesy of https://github.com/colin-kiegel/rust-pretty-assertions/issues/24
#[derive(PartialEq, Eq)]
#[doc(hidden)]
pub struct PrettyString<'a>(pub &'a str);

/// Make diff to display string as multi-line string
impl<'a> std::fmt::Debug for PrettyString<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

macro_rules! assert_eq {
    ($left:expr, $right:expr) => {
        pretty_assertions::assert_eq!(PrettyString($left.as_ref()), PrettyString($right.as_ref()));
    };
    ($left:expr, $right:expr, $message:expr) => {
        pretty_assertions::assert_eq!(
            PrettyString($left.as_ref()),
            PrettyString($right.as_ref()),
            $message
        );
    };
}

/// Loads an SWF and runs it through the Ruffle core for a number of frames.
/// Tests that the trace output matches the given expected output.
fn test_swf(
    swf_path: &str,
    num_frames: u32,
    expected_output_path: &str,
    check_img: bool,
) -> Result<(), Error> {
    test_swf_with_hooks(
        swf_path,
        num_frames,
        expected_output_path,
        |_| Ok(()),
        |_| Ok(()),
        check_img,
    )
}

/// Loads an SWF and runs it through the Ruffle core for a number of frames.
/// Tests that the trace output matches the given expected output.
fn test_swf_with_hooks(
    swf_path: &str,
    num_frames: u32,
    expected_output_path: &str,
    before_start: impl FnOnce(Arc<Mutex<Player>>) -> Result<(), Error>,
    before_end: impl FnOnce(Arc<Mutex<Player>>) -> Result<(), Error>,
    check_img: bool,
) -> Result<(), Error> {
    let mut expected_output = std::fs::read_to_string(expected_output_path)?.replace("\r\n", "\n");

    // Strip a trailing newline if it has one.
    if expected_output.ends_with('\n') {
        expected_output = expected_output[0..expected_output.len() - "\n".len()].to_string();
    }

    let trace_log = run_swf(swf_path, num_frames, before_start, before_end, check_img)?;
    assert_eq!(
        trace_log, expected_output,
        "ruffle output != flash player output"
    );

    Ok(())
}

/// Loads an SWF and runs it through the Ruffle core for a number of frames.
/// Tests that the trace output matches the given expected output.
/// If a line has a floating point value, it will be compared approxinmately using the given epsilon.
fn test_swf_approx(
    swf_path: &str,
    num_frames: u32,
    expected_output_path: &str,
    approx_assert_fn: impl Fn(f64, f64),
) -> Result<(), Error> {
    let trace_log = run_swf(swf_path, num_frames, |_| Ok(()), |_| Ok(()), false)?;
    let mut expected_data = std::fs::read_to_string(expected_output_path)?;

    // Strip a trailing newline if it has one.
    if expected_data.ends_with('\n') {
        expected_data = expected_data[0..expected_data.len() - "\n".len()].to_string();
    }

    std::assert_eq!(
        trace_log.lines().count(),
        expected_data.lines().count(),
        "# of lines of output didn't match"
    );

    for (actual, expected) in trace_log.lines().zip(expected_data.lines()) {
        // If these are numbers, compare using approx_eq.
        if let (Ok(actual), Ok(expected)) = (actual.parse::<f64>(), expected.parse::<f64>()) {
            // NaNs should be able to pass in an approx test.
            if actual.is_nan() && expected.is_nan() {
                continue;
            }

            // TODO: Lower this epsilon as the accuracy of the properties improves.
            // if let Some(relative_epsilon) = relative_epsilon {
            //     assert_relative_eq!(
            //         actual,
            //         expected,
            //         epsilon = absolute_epsilon,
            //         max_relative = relative_epsilon
            //     );
            // } else {
            //     assert_abs_diff_eq!(actual, expected, epsilon = absolute_epsilon);
            // }
            approx_assert_fn(actual, expected);
        } else {
            assert_eq!(actual, expected);
        }
    }
    Ok(())
}

/// Loads an SWF and runs it through the Ruffle core for a number of frames.
/// Tests that the trace output matches the given expected output.
fn run_swf(
    swf_path: &str,
    num_frames: u32,
    before_start: impl FnOnce(Arc<Mutex<Player>>) -> Result<(), Error>,
    before_end: impl FnOnce(Arc<Mutex<Player>>) -> Result<(), Error>,
    mut check_img: bool,
) -> Result<String, Error> {
    check_img &= RUN_IMG_TESTS;

    let base_path = Path::new(swf_path).parent().unwrap();
    let (mut executor, channel) = NullExecutor::new();
    let movie = SwfMovie::from_path(swf_path, None)?;
    let frame_time = 1000.0 / movie.frame_rate().to_f64();
    let trace_output = Rc::new(RefCell::new(Vec::new()));

    let mut platform_id = None;
    let backend_bit = wgpu::Backends::PRIMARY;

    let (render_backend, video_backend): (Box<dyn RenderBackend>, Box<dyn VideoBackend>) =
        if check_img {
            let instance = wgpu::Instance::new(backend_bit);

            let descriptors = WgpuRenderBackend::<TextureTarget>::build_descriptors(
                backend_bit,
                instance,
                None,
                Default::default(),
                None,
            )?;

            platform_id = Some(get_img_platform_suffix(&descriptors.info));

            let target = TextureTarget::new(
                &descriptors.device,
                (
                    movie.width().to_pixels() as u32,
                    movie.height().to_pixels() as u32,
                ),
            );

            let render_backend = Box::new(WgpuRenderBackend::new(descriptors, target)?);
            let video_backend = Box::new(SoftwareVideoBackend::new());
            (render_backend, video_backend)
        } else {
            (Box::new(NullRenderer), Box::new(NullVideoBackend::new()))
        };

    let player = Player::new(
        render_backend,
        Box::new(NullAudioBackend::new()),
        Box::new(NullNavigatorBackend::with_base_path(base_path, channel)),
        Box::new(MemoryStorageBackend::default()),
        Box::new(NullLocaleBackend::new()),
        video_backend,
        Box::new(TestLogBackend::new(trace_output.clone())),
        Box::new(NullUiBackend::new()),
    )?;
    player.lock().unwrap().set_root_movie(Arc::new(movie));
    player
        .lock()
        .unwrap()
        .set_max_execution_duration(Duration::from_secs(300));

    before_start(player.clone())?;

    for _ in 0..num_frames {
        player.lock().unwrap().run_frame();
        player.lock().unwrap().update_timers(frame_time);
        executor.poll_all().unwrap();
    }

    // Render the image to disk
    // FIXME: Determine how we want to compare against on on-disk image
    if check_img {
        player.lock().unwrap().render();
        let mut player_lock = player.lock().unwrap();
        let renderer = player_lock
            .renderer_mut()
            .downcast_mut::<WgpuRenderBackend<TextureTarget>>()
            .unwrap();
        let target = renderer.target();
        let image = target
            .capture(renderer.device())
            .expect("Failed to capture image");

        // The swf path ends in '<swf_name>/test.swf' - extract `swf_name`
        let mut swf_path_buf = PathBuf::from(swf_path);
        swf_path_buf.pop();

        let swf_name = swf_path_buf.file_name().unwrap().to_string_lossy();
        let img_name = format!("{}-{}.png", swf_name, platform_id.unwrap());

        let mut img_path = swf_path_buf.clone();
        img_path.push(&img_name);

        let result = match image::open(&img_path) {
            Ok(existing_img) => {
                if existing_img
                    .as_rgba8()
                    .expect("Expected 8-bit RGBA image")
                    .as_raw()
                    == image.as_raw()
                {
                    Ok(())
                } else {
                    Err(format!(
                        "Test output does not match existing image `{:?}`",
                        img_path
                    ))
                }
            }
            Err(err) => Err(format!(
                "Error occured when trying to read existing image `{:?}`: {}",
                img_path, err
            )),
        };

        if let Err(err) = result {
            let new_img_path = img_path.with_file_name(img_name + ".updated");
            image.save_with_format(&new_img_path, image::ImageFormat::Png)?;
            panic!(
                "Image test failed - saved new image to `{:?}`\n{}",
                new_img_path, err
            );
        }
    }

    before_end(player)?;

    executor.block_all().unwrap();

    let trace = trace_output.borrow().join("\n");
    Ok(trace)
}

struct TestLogBackend {
    trace_output: Rc<RefCell<Vec<String>>>,
}

impl TestLogBackend {
    pub fn new(trace_output: Rc<RefCell<Vec<String>>>) -> Self {
        Self { trace_output }
    }
}

impl LogBackend for TestLogBackend {
    fn avm_trace(&self, message: &str) {
        self.trace_output.borrow_mut().push(message.to_string());
    }
}

#[derive(Default)]
pub struct ExternalInterfaceTestProvider {}

impl ExternalInterfaceTestProvider {
    pub fn new() -> Self {
        Default::default()
    }
}

fn do_trace(context: &mut UpdateContext<'_, '_, '_>, args: &[ExternalValue]) -> ExternalValue {
    context
        .log
        .avm_trace(&format!("[ExternalInterface] trace: {:?}", args));
    "Traced!".into()
}

fn do_ping(context: &mut UpdateContext<'_, '_, '_>, _args: &[ExternalValue]) -> ExternalValue {
    context.log.avm_trace("[ExternalInterface] ping");
    "Pong!".into()
}

fn do_reentry(context: &mut UpdateContext<'_, '_, '_>, _args: &[ExternalValue]) -> ExternalValue {
    context
        .log
        .avm_trace("[ExternalInterface] starting reentry");
    if let Some(callback) = context.external_interface.get_callback("callWith") {
        callback.call(
            context,
            "callWith",
            vec!["trace".into(), "successful reentry!".into()],
        )
    } else {
        ExternalValue::Null
    }
}

impl ExternalInterfaceProvider for ExternalInterfaceTestProvider {
    fn get_method(&self, name: &str) -> Option<Box<dyn ExternalInterfaceMethod>> {
        match name {
            "trace" => Some(Box::new(do_trace)),
            "ping" => Some(Box::new(do_ping)),
            "reentry" => Some(Box::new(do_reentry)),
            _ => None,
        }
    }

    fn on_callback_available(&self, _name: &str) {}

    fn on_fs_command(&self, _command: &str, _args: &str) -> bool {
        false
    }
}
