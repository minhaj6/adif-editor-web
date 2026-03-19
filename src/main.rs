use dioxus::prelude::*;
use std::collections::HashMap;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast, JsValue};

#[cfg(target_arch = "wasm32")]
use web_sys::{
    Blob, BlobPropertyBag, DragEvent, Event, File, FileReader, HtmlAnchorElement,
    HtmlInputElement, Url,
};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

const DROP_ZONE_ID: &str = "file-drop-zone";
const FILE_INPUT_ID: &str = "file-input";

fn main() {
    dioxus::launch(App);
}

#[derive(Clone, PartialEq)]
struct AdifDocument {
    header: String,
    records: Vec<AdifRecord>,
}

#[derive(Clone, PartialEq)]
struct AdifRecord {
    fields: Vec<AdifField>,
}

#[derive(Clone, PartialEq)]
struct AdifField {
    name: String,
    value: String,
    field_type: String,
}

#[derive(Clone, PartialEq)]
enum Selection {
    None,
    Cells(CellRange),
    Rows(IndexRange),
    Columns(IndexRange),
}

#[derive(Clone, PartialEq)]
struct CellRange {
    start_row: usize,
    end_row: usize,
    start_col: usize,
    end_col: usize,
}

#[derive(Clone, PartialEq)]
struct IndexRange {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum SelectionMode {
    Cells,
    Rows,
    Columns,
}

#[derive(Clone, Copy, PartialEq)]
struct DragSelection {
    mode: SelectionMode,
    anchor_row: usize,
    anchor_col: usize,
}

#[derive(Clone, PartialEq)]
struct ColumnResizeState {
    column_name: String,
    start_x: f64,
    start_width: f64,
}

#[component]
fn App() -> Element {
    let mut document = use_signal(|| None::<AdifDocument>);
    let status = use_signal(String::new);
    let active_file_name = use_signal(|| "edited-log.adif".to_string());
    let mut new_column_name = use_signal(String::new);
    let mut replace_value = use_signal(String::new);
    let mut selection = use_signal(|| Selection::None);
    let mut drag_selection = use_signal(|| None::<DragSelection>);
    let mut manual_column_widths = use_signal(HashMap::<String, f64>::new);
    let mut resize_state = use_signal(|| None::<ColumnResizeState>);

    #[cfg(target_arch = "wasm32")]
    use_effect({
        let document = document;
        let status = status;
        let active_file_name = active_file_name;

        move || {
            install_file_handlers(document, status, active_file_name);
        }
    });

    let current_document = document();
    let summary = current_document.as_ref().map(|doc| {
        let mut field_count = 0usize;
        for record in &doc.records {
            field_count += record.fields.len();
        }
        let column_count = ordered_columns(doc).len();
        (doc.records.len(), field_count, column_count)
    });
    let table_columns = current_document
        .as_ref()
        .map(ordered_columns)
        .unwrap_or_default();
    let add_row_columns = table_columns.clone();
    let replace_columns = table_columns.clone();
    let clear_columns = table_columns.clone();
    let delete_columns = table_columns.clone();
    let resolved_column_widths = resolve_column_widths(
        current_document.as_ref(),
        &table_columns,
        &manual_column_widths(),
    );

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        div {
            class: "page",
            onmousemove: move |event| {
                if let Some(active_resize) = resize_state() {
                    update_column_width(
                        &mut manual_column_widths,
                        &active_resize,
                        mouse_client_x(&event),
                    );
                }
            },
            onmouseup: move |_| {
                drag_selection.set(None);
                resize_state.set(None);
            },
            section { class: "hero",
                p { class: "eyebrow", "KQ4VPZ's ADIF Editor" }
                h3 { "Edit ADIF logs in a spreadsheet-like web-editor." }
                p { class: "lede",
                    "Inspired by legendary tool adif-master, but not limited to the microslop platform."
                }
            }

            section { class: "panel upload-panel",
                div { class: "upload-layout",
                    div { id: DROP_ZONE_ID, class: "drop-zone", tabindex: "0",
                        h2 { "Import log" }
                        p { "Drag and drop a `.adi` or `.adif` file here." }
                        p { class: "drop-hint", "You can also use the file picker if that's easier." }
                        button {
                            class: "secondary-button",
                            onclick: move |_| open_file_picker(),
                            "Choose file"
                        }
                    }

                    div { class: "upload-sidecar",
                        h2 { "Workflow" }
                        p {
                            "Load a file, edit directly in the grid, add rows or columns, then export the updated ADIF back to disk."
                        }
                    }
                }

                input {
                    id: FILE_INPUT_ID,
                    r#type: "file",
                    accept: ".adi,.adif,text/plain",
                    hidden: true,
                }

                if !status().is_empty() {
                    p { class: "status-message", "{status}" }
                }

                if let Some((record_count, field_count, column_count)) = summary {
                    div { class: "summary-grid",
                        div { class: "summary-card",
                            span { class: "summary-label", "File" }
                            strong { "{active_file_name}" }
                        }
                        div { class: "summary-card",
                            span { class: "summary-label", "Records" }
                            strong { "{record_count}" }
                        }
                        div { class: "summary-card",
                            span { class: "summary-label", "Fields" }
                            strong { "{field_count}" }
                        }
                        div { class: "summary-card",
                            span { class: "summary-label", "Columns" }
                            strong { "{column_count}" }
                        }
                    }
                }
            }

            if let Some(current_document) = current_document.clone() {
                section { class: "panel editor-panel",
                    div { class: "toolbar",
                        div {
                            h2 { "Log grid" }
                            p { class: "toolbar-copy",
                                "Rows behave like QSOs and columns map to ADIF tags."
                            }
                        }
                        div { class: "toolbar-actions",
                            button {
                                class: "primary-button",
                                onclick: {
                                    let document = current_document.clone();
                                    let file_name = export_name(&active_file_name());
                                    move |_| download_adif(&document, &file_name)
                                },
                                "Export ADIF"
                            }
                        }
                    }

                    div { class: "grid-controls",
                        div { class: "grid-actions-card",
                            label { class: "field-label", "Grid actions" }
                            div { class: "grid-actions-inline",
                                button {
                                    class: "secondary-button",
                                    onclick: move |_| {
                                        let columns = add_row_columns.clone();
                                        document
                                            .with_mut(|maybe_doc| {
                                                if let Some(doc) = maybe_doc.as_mut() {
                                                    doc.records.push(default_record_with_columns(&columns));
                                                }
                                            });
                                    },
                                    "Add row"
                                }
                                input {
                                    value: new_column_name(),
                                    placeholder: "Example: RST_RCVD or MY_GRIDSQUARE",
                                    oninput: move |event| new_column_name.set(event.value()),
                                }
                                button {
                                    class: "secondary-button",
                                    onclick: move |_| {
                                        let column_name = new_column_name();
                                        add_column(&mut document, &column_name);
                                        new_column_name.set(String::new());
                                    },
                                    "Insert column"
                                }
                            }
                        }
                        div { class: "header-editor compact-header",
                            label { class: "field-label", "ADIF header" }
                            textarea {
                                class: "header-textarea",
                                value: current_document.header.clone(),
                                placeholder: "Optional ADIF header metadata",
                                oninput: move |event| {
                                    let value = event.value();
                                    document
                                        .with_mut(|maybe_doc| {
                                            if let Some(doc) = maybe_doc.as_mut() {
                                                doc.header = value.clone();
                                            }
                                        });
                                },
                            }
                        }
                    }

                    div { class: "selection-bar",
                        div { class: "selection-copy",
                            strong { "Selection" }
                            span { "{selection_label(&selection(), &table_columns)}" }
                        }
                        div { class: "selection-actions",
                            input {
                                value: replace_value(),
                                placeholder: "Replacement value for selected cells or columns",
                                oninput: move |event| replace_value.set(event.value()),
                            }
                            button {
                                class: "secondary-button",
                                onclick: {
                                    let columns = replace_columns.clone();
                                    let selection_snapshot = selection();
                                    let replacement = replace_value();
                                    move |_| {
                                        replace_selected_values(
                                            &mut document,
                                            &selection_snapshot,
                                            &columns,
                                            &replacement,
                                        );
                                    }
                                },
                                disabled: matches!(selection(), Selection::None),
                                "Replace selection"
                            }
                            button {
                                class: "secondary-button",
                                onclick: {
                                    let columns = clear_columns.clone();
                                    let selection_snapshot = selection();
                                    move |_| {
                                        clear_selected_values(&mut document, &selection_snapshot, &columns);
                                    }
                                },
                                disabled: matches!(selection(), Selection::None),
                                "Clear values"
                            }
                            button {
                                class: "danger-button",
                                onclick: {
                                    let columns = delete_columns.clone();
                                    let selection_snapshot = selection();
                                    let mut selection = selection;
                                    move |_| {
                                        delete_selection(&mut document, &selection_snapshot, &columns);
                                        selection.set(Selection::None);
                                    }
                                },
                                disabled: matches!(selection(), Selection::None),
                                "Delete selection"
                            }
                            button {
                                class: "secondary-button",
                                onclick: move |_| selection.set(Selection::None),
                                disabled: matches!(selection(), Selection::None),
                                "Clear selection"
                            }
                        }
                    }

                    div { class: "table-shell",
                        if current_document.records.is_empty() {
                            div { class: "empty-state",
                                h3 { "No rows yet" }
                                p { "Import a file or add a blank row to start editing in the grid." }
                            }
                        } else {
                            div { class: "grid-legend",
                                span { class: "legend-chip", "Sticky row numbers" }
                                span { class: "legend-chip", "Horizontal scroll" }
                                span { class: "legend-chip", "In-cell editing" }
                            }
                            div { class: "table-scroll",
                                table { class: "log-table",
                                    thead {
                                        tr {
                                            th { class: "row-header sticky-col", "#" }
                                            for ((column_index , column_name) , width) in table_columns.iter().enumerate().zip(resolved_column_widths.iter().copied()) {
                                                th {
                                                    class: column_header_class(&selection(), column_index),
                                                    style: column_style(width),
                                                    onmousedown: move |_| {
                                                        start_column_selection(&mut selection, &mut drag_selection, column_index);
                                                    },
                                                    onmouseenter: move |_| {
                                                        let drag_state = drag_selection();
                                                        extend_drag_selection(&mut selection, &drag_state, 0, column_index);
                                                    },
                                                    div { class: "column-head",
                                                        strong { "{column_name}" }
                                                        if let Some(field_type) = common_field_type(column_name) {
                                                            span { class: "type-badge",
                                                                "{field_type}"
                                                            }
                                                        }
                                                        div {
                                                            class: "column-resizer",
                                                            onmousedown: {
                                                                let column_name = column_name.clone();
                                                                move |event| {
                                                                    event.stop_propagation();
                                                                    resize_state
                                                                        .set(
                                                                            Some(ColumnResizeState {
                                                                                column_name: column_name.clone(),
                                                                                start_x: mouse_client_x(&event),
                                                                                start_width: width,
                                                                            }),
                                                                        );
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    tbody {
                                        for (record_index , record) in current_document.records.iter().enumerate() {
                                            tr {
                                                td {
                                                    class: row_header_class(is_row_selected(&selection(), record_index)),
                                                    onmousedown: move |_| {
                                                        start_row_selection(&mut selection, &mut drag_selection, record_index);
                                                    },
                                                    onmouseenter: move |_| {
                                                        let drag_state = drag_selection();
                                                        extend_drag_selection(&mut selection, &drag_state, record_index, 0);
                                                    },
                                                    div { class: "row-index", "{record_index + 1}" }
                                                    button {
                                                        class: "danger-button compact-button row-delete",
                                                        onclick: move |_| remove_record(document, record_index),
                                                        "Delete"
                                                    }
                                                }
                                                for ((column_index , column_name) , width) in table_columns.iter().enumerate().zip(resolved_column_widths.iter().copied()) {
                                                    td {
                                                        class: cell_class(&selection(), record_index, column_index),
                                                        style: column_style(width),
                                                        onmousedown: move |_| {
                                                            start_cell_selection(
                                                                &mut selection,
                                                                &mut drag_selection,
                                                                record_index,
                                                                column_index,
                                                            );
                                                        },
                                                        onmouseenter: move |_| {
                                                            let drag_state = drag_selection();
                                                            extend_drag_selection(&mut selection, &drag_state, record_index, column_index);
                                                        },
                                                        input {
                                                            class: cell_input_class(column_name),
                                                            style: input_style(width),
                                                            value: field_value(record, column_name),
                                                            placeholder: column_name.clone(),
                                                            onmousedown: move |_event| {
                                                                start_cell_selection(
                                                                    &mut selection,
                                                                    &mut drag_selection,
                                                                    record_index,
                                                                    column_index,
                                                                );
                                                            },
                                                            oninput: {
                                                                let column_name = column_name.clone();
                                                                move |event| {
                                                                    set_cell_value(document, record_index, &column_name, event.value());
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                section { class: "panel empty-panel",
                    h2 { "Ready for your first log" }
                    p {
                        "Once you drop an ADIF file, the records will appear here as an editable table with row-and-column style log editing."
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ColumnKind {
    Date,
    Time,
    Frequency,
    Numeric,
    Text,
}

fn selection_label(selection: &Selection, columns: &[String]) -> String {
    match selection {
        Selection::None => "No active selection".to_string(),
        Selection::Cells(range) => format!(
            "Cells R{}:R{} x {}:{}",
            range.start_row + 1,
            range.end_row + 1,
            column_label(columns, range.start_col),
            column_label(columns, range.end_col)
        ),
        Selection::Rows(range) => format!("Rows {} to {}", range.start + 1, range.end + 1),
        Selection::Columns(range) => format!(
            "Columns {} to {}",
            column_label(columns, range.start),
            column_label(columns, range.end)
        ),
    }
}

fn column_label(columns: &[String], index: usize) -> String {
    columns
        .get(index)
        .cloned()
        .unwrap_or_else(|| format!("Column {}", index + 1))
}

fn normalize_index_range(start: usize, end: usize) -> IndexRange {
    if start <= end {
        IndexRange { start, end }
    } else {
        IndexRange {
            start: end,
            end: start,
        }
    }
}

fn normalize_cell_range(
    start_row: usize,
    end_row: usize,
    start_col: usize,
    end_col: usize,
) -> CellRange {
    let rows = normalize_index_range(start_row, end_row);
    let cols = normalize_index_range(start_col, end_col);

    CellRange {
        start_row: rows.start,
        end_row: rows.end,
        start_col: cols.start,
        end_col: cols.end,
    }
}

fn start_cell_selection(
    selection: &mut Signal<Selection>,
    drag_selection: &mut Signal<Option<DragSelection>>,
    row: usize,
    col: usize,
) {
    selection.set(Selection::Cells(CellRange {
        start_row: row,
        end_row: row,
        start_col: col,
        end_col: col,
    }));
    drag_selection.set(Some(DragSelection {
        mode: SelectionMode::Cells,
        anchor_row: row,
        anchor_col: col,
    }));
}

fn start_row_selection(
    selection: &mut Signal<Selection>,
    drag_selection: &mut Signal<Option<DragSelection>>,
    row: usize,
) {
    selection.set(Selection::Rows(IndexRange { start: row, end: row }));
    drag_selection.set(Some(DragSelection {
        mode: SelectionMode::Rows,
        anchor_row: row,
        anchor_col: 0,
    }));
}

fn start_column_selection(
    selection: &mut Signal<Selection>,
    drag_selection: &mut Signal<Option<DragSelection>>,
    col: usize,
) {
    selection.set(Selection::Columns(IndexRange { start: col, end: col }));
    drag_selection.set(Some(DragSelection {
        mode: SelectionMode::Columns,
        anchor_row: 0,
        anchor_col: col,
    }));
}

fn extend_drag_selection(
    selection: &mut Signal<Selection>,
    drag_selection: &Option<DragSelection>,
    row: usize,
    col: usize,
) {
    let Some(drag) = drag_selection else {
        return;
    };

    match drag.mode {
        SelectionMode::Cells => selection.set(Selection::Cells(normalize_cell_range(
            drag.anchor_row,
            row,
            drag.anchor_col,
            col,
        ))),
        SelectionMode::Rows => {
            selection.set(Selection::Rows(normalize_index_range(drag.anchor_row, row)))
        }
        SelectionMode::Columns => {
            selection.set(Selection::Columns(normalize_index_range(drag.anchor_col, col)))
        }
    }
}

fn is_cell_selected(selection: &Selection, row: usize, col: usize) -> bool {
    match selection {
        Selection::Cells(range) => {
            row >= range.start_row
                && row <= range.end_row
                && col >= range.start_col
                && col <= range.end_col
        }
        Selection::Rows(range) => row >= range.start && row <= range.end,
        Selection::Columns(range) => col >= range.start && col <= range.end,
        Selection::None => false,
    }
}

fn is_row_selected(selection: &Selection, row: usize) -> bool {
    match selection {
        Selection::Rows(range) => row >= range.start && row <= range.end,
        Selection::Cells(range) => row >= range.start_row && row <= range.end_row,
        Selection::None | Selection::Columns(_) => false,
    }
}

fn is_column_selected(selection: &Selection, col: usize) -> bool {
    match selection {
        Selection::Columns(range) => col >= range.start && col <= range.end,
        Selection::Cells(range) => col >= range.start_col && col <= range.end_col,
        Selection::None | Selection::Rows(_) => false,
    }
}

fn row_header_class(selected: bool) -> &'static str {
    if selected {
        "row-header sticky-col selection-active"
    } else {
        "row-header sticky-col"
    }
}

fn column_header_class(selection: &Selection, col: usize) -> &'static str {
    if is_column_selected(selection, col) {
        "selection-active"
    } else {
        ""
    }
}

fn cell_class(selection: &Selection, row: usize, col: usize) -> &'static str {
    if is_cell_selected(selection, row, col) {
        "selection-active"
    } else {
        ""
    }
}

fn resolve_column_widths(
    document: Option<&AdifDocument>,
    columns: &[String],
    manual_widths: &HashMap<String, f64>,
) -> Vec<f64> {
    columns
        .iter()
        .map(|column_name| {
            manual_widths
                .get(column_name)
                .copied()
                .unwrap_or_else(|| suggested_column_width(document, column_name))
        })
        .collect()
}

fn suggested_column_width(document: Option<&AdifDocument>, column_name: &str) -> f64 {
    let value_len = document
        .map(|doc| {
            doc.records
                .iter()
                .map(|record| field_value(record, column_name).chars().count())
                .max()
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let header_width = header_display_width(column_name);
    let value_width = value_display_width(column_name, value_len);
    header_width.max(value_width)
}

fn header_display_width(column_name: &str) -> f64 {
    let text_width = column_name.chars().count() as f64 * 7.8;
    let badge_width = if common_field_type(column_name).is_some() {
        30.0
    } else {
        0.0
    };

    (text_width + badge_width + 34.0).clamp(84.0, 260.0)
}

fn value_display_width(column_name: &str, value_len: usize) -> f64 {
    let value_len = value_len as f64;

    match classify_column(column_name) {
        ColumnKind::Date => (value_len * 7.0 + 18.0).clamp(88.0, 118.0),
        ColumnKind::Time => (value_len * 6.5 + 18.0).clamp(76.0, 104.0),
        ColumnKind::Frequency => (value_len * 7.0 + 20.0).clamp(82.0, 132.0),
        ColumnKind::Numeric => (value_len * 7.0 + 20.0).clamp(74.0, 116.0),
        ColumnKind::Text => (value_len * 7.6 + 24.0).clamp(78.0, 220.0),
    }
}

fn update_column_width(
    manual_widths: &mut Signal<HashMap<String, f64>>,
    resize_state: &ColumnResizeState,
    current_x: f64,
) {
    let next_width = (resize_state.start_width + (current_x - resize_state.start_x)).clamp(64.0, 320.0);
    manual_widths.with_mut(|widths| {
        widths.insert(resize_state.column_name.clone(), next_width);
    });
}

fn mouse_client_x(event: &MouseEvent) -> f64 {
    event.client_coordinates().x
}

fn column_style(width: f64) -> String {
    format!("width: {width}px; min-width: {width}px; max-width: {width}px;")
}

fn input_style(width: f64) -> String {
    format!("width: {width}px; min-width: {width}px; max-width: {width}px;")
}

fn ordered_columns(document: &AdifDocument) -> Vec<String> {
    let preferred = [
        "CALL",
        "BAND",
        "FREQ",
        "MODE",
        "SUBMODE",
        "QSO_DATE",
        "TIME_ON",
        "TIME_OFF",
        "RST_SENT",
        "RST_RCVD",
        "NAME",
        "COUNTRY",
        "STATE",
        "GRIDSQUARE",
        "MY_POTA_REF",
        "MY_SIG_INFO",
        "MY_GRIDSQUARE",
        "COMMENT",
    ];

    let mut columns = Vec::new();

    for preferred_name in preferred {
        if document
            .records
            .iter()
            .any(|record| has_field(record, preferred_name))
        {
            columns.push(preferred_name.to_string());
        }
    }

    for record in &document.records {
        for field in &record.fields {
            let normalized = normalize_field_name(&field.name);
            if !normalized.is_empty() && !columns.iter().any(|existing| existing == &normalized) {
                columns.push(normalized);
            }
        }
    }

    if columns.is_empty() {
        default_columns()
    } else {
        columns
    }
}

fn default_columns() -> Vec<String> {
    vec![
        "CALL".to_string(),
        "BAND".to_string(),
        "MODE".to_string(),
        "QSO_DATE".to_string(),
        "TIME_ON".to_string(),
    ]
}

fn default_record_with_columns(columns: &[String]) -> AdifRecord {
    let template_columns = if columns.is_empty() {
        default_columns()
    } else {
        columns.to_vec()
    };

    AdifRecord {
        fields: template_columns
            .into_iter()
            .map(|name| AdifField {
                field_type: default_field_type(&name).to_string(),
                name,
                value: String::new(),
            })
            .collect(),
    }
}

fn normalize_field_name(input: &str) -> String {
    input.trim().replace(' ', "_").to_ascii_uppercase()
}

fn default_field_type(field_name: &str) -> &'static str {
    match normalize_field_name(field_name).as_str() {
        "QSO_DATE" | "QSLSDATE" | "QSLRDATE" | "LOTW_QSLSDATE" | "LOTW_QSLRDATE"
        | "EQSL_QSLSDATE" | "EQSL_QSLRDATE" => "D",
        "TIME_ON" | "TIME_OFF" => "T",
        _ => "",
    }
}

fn common_field_type(field_name: &str) -> Option<&'static str> {
    let field_type = default_field_type(field_name);
    if field_type.is_empty() {
        None
    } else {
        Some(field_type)
    }
}

fn classify_column(field_name: &str) -> ColumnKind {
    match normalize_field_name(field_name).as_str() {
        "QSO_DATE" | "QSLSDATE" | "QSLRDATE" | "LOTW_QSLSDATE" | "LOTW_QSLRDATE"
        | "EQSL_QSLSDATE" | "EQSL_QSLRDATE" => ColumnKind::Date,
        "TIME_ON" | "TIME_OFF" => ColumnKind::Time,
        "FREQ" | "FREQ_RX" => ColumnKind::Frequency,
        "CQZ" | "ITUZ" | "TX_PWR" => ColumnKind::Numeric,
        _ => ColumnKind::Text,
    }
}

fn cell_input_class(field_name: &str) -> &'static str {
    match classify_column(field_name) {
        ColumnKind::Date => "cell-input cell-date",
        ColumnKind::Time => "cell-input cell-time",
        ColumnKind::Frequency => "cell-input cell-frequency",
        ColumnKind::Numeric => "cell-input cell-number",
        ColumnKind::Text => "cell-input",
    }
}

fn export_name(file_name: &str) -> String {
    if let Some((base, extension)) = file_name.rsplit_once('.') {
        format!("{base}-edited.{extension}")
    } else {
        format!("{file_name}-edited.adif")
    }
}

fn has_field(record: &AdifRecord, field_name: &str) -> bool {
    let target = normalize_field_name(field_name);
    record
        .fields
        .iter()
        .any(|field| normalize_field_name(&field.name) == target)
}

fn field_value(record: &AdifRecord, field_name: &str) -> String {
    let target = normalize_field_name(field_name);
    record
        .fields
        .iter()
        .find(|field| normalize_field_name(&field.name) == target)
        .map(|field| field.value.clone())
        .unwrap_or_default()
}

fn set_cell_value(
    mut document: Signal<Option<AdifDocument>>,
    record_index: usize,
    field_name: &str,
    value: String,
) {
    let normalized = normalize_field_name(field_name);

    document.with_mut(|maybe_doc| {
        if let Some(doc) = maybe_doc.as_mut() {
            if let Some(record) = doc.records.get_mut(record_index) {
                if let Some(field) = record
                    .fields
                    .iter_mut()
                    .find(|field| normalize_field_name(&field.name) == normalized)
                {
                    field.value = value.clone();
                    return;
                }

                if value.is_empty() {
                    return;
                }

                record.fields.push(AdifField {
                    name: normalized.clone(),
                    value: value.clone(),
                    field_type: default_field_type(&normalized).to_string(),
                });
            }
        }
    });
}

fn replace_selected_values(
    document: &mut Signal<Option<AdifDocument>>,
    selection: &Selection,
    columns: &[String],
    replacement: &str,
) {
    document.with_mut(|maybe_doc| {
        let Some(doc) = maybe_doc.as_mut() else {
            return;
        };

        match selection {
            Selection::None => {}
            Selection::Cells(range) => {
                for row in range.start_row..=range.end_row {
                    for col in range.start_col..=range.end_col {
                        if let Some(column_name) = columns.get(col) {
                            set_field_value_on_record(
                                doc.records.get_mut(row),
                                column_name,
                                replacement.to_string(),
                            );
                        }
                    }
                }
            }
            Selection::Rows(range) => {
                for row in range.start..=range.end {
                    if let Some(record) = doc.records.get_mut(row) {
                        for field in &mut record.fields {
                            field.value = replacement.to_string();
                        }
                    }
                }
            }
            Selection::Columns(range) => {
                for col in range.start..=range.end {
                    if let Some(column_name) = columns.get(col) {
                        for record in &mut doc.records {
                            set_field_value_on_record(Some(record), column_name, replacement.to_string());
                        }
                    }
                }
            }
        }
    });
}

fn clear_selected_values(
    document: &mut Signal<Option<AdifDocument>>,
    selection: &Selection,
    columns: &[String],
) {
    replace_selected_values(document, selection, columns, "");
}

fn delete_selection(
    document: &mut Signal<Option<AdifDocument>>,
    selection: &Selection,
    columns: &[String],
) {
    document.with_mut(|maybe_doc| {
        let Some(doc) = maybe_doc.as_mut() else {
            return;
        };

        match selection {
            Selection::None => {}
            Selection::Cells(range) => {
                for row in range.start_row..=range.end_row {
                    for col in range.start_col..=range.end_col {
                        if let Some(column_name) = columns.get(col) {
                            set_field_value_on_record(doc.records.get_mut(row), column_name, String::new());
                        }
                    }
                }
            }
            Selection::Rows(range) => {
                doc.records.drain(range.start..=range.end);
            }
            Selection::Columns(range) => {
                let selected_columns: Vec<String> = (range.start..=range.end)
                    .filter_map(|col| columns.get(col).cloned())
                    .collect();

                for record in &mut doc.records {
                    record.fields.retain(|field| {
                        !selected_columns
                            .iter()
                            .any(|column| normalize_field_name(column) == normalize_field_name(&field.name))
                    });
                }
            }
        }
    });
}

fn set_field_value_on_record(record: Option<&mut AdifRecord>, field_name: &str, value: String) {
    let Some(record) = record else {
        return;
    };

    let normalized = normalize_field_name(field_name);
    if let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| normalize_field_name(&field.name) == normalized)
    {
        field.value = value;
        return;
    }

    record.fields.push(AdifField {
        name: normalized.clone(),
        value,
        field_type: default_field_type(&normalized).to_string(),
    });
}

fn add_column(document: &mut Signal<Option<AdifDocument>>, field_name: &str) {
    let normalized = normalize_field_name(field_name);
    if normalized.is_empty() {
        return;
    }

    document.with_mut(|maybe_doc| {
        if let Some(doc) = maybe_doc.as_mut() {
            if doc.records.is_empty() {
                doc.records.push(AdifRecord {
                    fields: vec![AdifField {
                        name: normalized.clone(),
                        value: String::new(),
                        field_type: default_field_type(&normalized).to_string(),
                    }],
                });
                return;
            }

            for record in &mut doc.records {
                if !has_field(record, &normalized) {
                    record.fields.push(AdifField {
                        name: normalized.clone(),
                        value: String::new(),
                        field_type: default_field_type(&normalized).to_string(),
                    });
                }
            }
        }
    });
}

fn remove_record(mut document: Signal<Option<AdifDocument>>, record_index: usize) {
    document.with_mut(|maybe_doc| {
        if let Some(doc) = maybe_doc.as_mut() {
            if record_index < doc.records.len() {
                doc.records.remove(record_index);
            }
        }
    });
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn parse_adif(input: &str) -> Result<AdifDocument, String> {
    let lower = input.to_ascii_lowercase();
    let (header, body) = if let Some(index) = lower.find("<eoh>") {
        let header = input[..index].trim().to_string();
        let body_start = index + "<eoh>".len();
        (header, &input[body_start..])
    } else {
        (String::new(), input)
    };

    let mut records = Vec::new();
    let mut current_fields = Vec::new();
    let mut cursor = 0usize;
    let bytes = body.as_bytes();

    while cursor < bytes.len() {
        let Some(relative_start) = body[cursor..].find('<') else {
            break;
        };

        cursor += relative_start;

        let Some(relative_end) = body[cursor..].find('>') else {
            return Err("Found an ADIF tag without a closing `>`.".to_string());
        };

        let tag_end = cursor + relative_end;
        let tag_contents = body[cursor + 1..tag_end].trim();
        let next_value_start = tag_end + 1;
        let normalized_tag = tag_contents.to_ascii_lowercase();

        if normalized_tag == "eor" {
            if !current_fields.is_empty() {
                records.push(AdifRecord {
                    fields: std::mem::take(&mut current_fields),
                });
            }
            cursor = next_value_start;
            continue;
        }

        if normalized_tag == "eoh" {
            cursor = next_value_start;
            continue;
        }

        let mut pieces = tag_contents.split(':');
        let Some(name) = pieces.next() else {
            return Err("Encountered an empty ADIF field name.".to_string());
        };
        let Some(length_part) = pieces.next() else {
            cursor = next_value_start;
            continue;
        };

        let field_length = length_part
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("Invalid ADIF field length in tag `<{tag_contents}>`."))?;

        let field_type = pieces.next().unwrap_or_default().trim().to_string();
        let value_end = next_value_start + field_length;

        if value_end > body.len() {
            return Err(format!(
                "Field `{}` declares length {} but the file ends early.",
                name.trim(),
                field_length
            ));
        }

        current_fields.push(AdifField {
            name: name.trim().to_ascii_uppercase(),
            value: body[next_value_start..value_end].to_string(),
            field_type,
        });

        cursor = value_end;
    }

    if !current_fields.is_empty() {
        records.push(AdifRecord {
            fields: current_fields,
        });
    }

    Ok(AdifDocument { header, records })
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn serialize_adif(document: &AdifDocument) -> String {
    let mut output = String::new();

    if !document.header.trim().is_empty() {
        output.push_str(document.header.trim_end());
        output.push('\n');
    }

    output.push_str("<EOH>\n");

    for record in &document.records {
        for field in &record.fields {
            let field_name = field.name.trim().to_ascii_uppercase();

            if field_name.is_empty() {
                continue;
            }

            let value_length = field.value.len();
            let field_type = field.field_type.trim();

            if field_type.is_empty() {
                output.push_str(&format!("<{}:{}>{}", field_name, value_length, field.value));
            } else {
                output.push_str(&format!(
                    "<{}:{}:{}>{}",
                    field_name,
                    value_length,
                    field_type.to_ascii_uppercase(),
                    field.value
                ));
            }
        }

        output.push_str("<EOR>\n");
    }

    output
}

#[cfg(target_arch = "wasm32")]
fn install_file_handlers(
    document_signal: Signal<Option<AdifDocument>>,
    mut status_signal: Signal<String>,
    file_name_signal: Signal<String>,
) {
    let Some(window) = web_sys::window() else {
        status_signal.set("Browser window was not available.".to_string());
        return;
    };

    let Some(document) = window.document() else {
        status_signal.set("Browser document was not available.".to_string());
        return;
    };

    let Some(drop_zone) = document.get_element_by_id(DROP_ZONE_ID) else {
        status_signal.set("Drop zone element was not found.".to_string());
        return;
    };

    let Some(input_element) = document.get_element_by_id(FILE_INPUT_ID) else {
        status_signal.set("File input element was not found.".to_string());
        return;
    };

    let Ok(file_input) = input_element.dyn_into::<HtmlInputElement>() else {
        status_signal.set("The file input element could not be initialized.".to_string());
        return;
    };

    {
        let drag_over = Closure::<dyn FnMut(DragEvent)>::wrap(Box::new(move |event: DragEvent| {
            event.prevent_default();
        }));

        let _ = drop_zone
            .add_event_listener_with_callback("dragover", drag_over.as_ref().unchecked_ref());
        drag_over.forget();
    }

    {
        let document_signal = document_signal;
        let mut status_signal = status_signal;
        let file_name_signal = file_name_signal;

        let drop_handler =
            Closure::<dyn FnMut(DragEvent)>::wrap(Box::new(move |event: DragEvent| {
                event.prevent_default();

                let Some(data_transfer) = event.data_transfer() else {
                    status_signal.set("No dropped file data was available.".to_string());
                    return;
                };

                let Some(files) = data_transfer.files() else {
                    status_signal.set("The drop action did not include files.".to_string());
                    return;
                };

                if files.length() == 0 {
                    status_signal.set("Drop an ADIF file to continue.".to_string());
                    return;
                }

                let Some(file) = files.get(0) else {
                    status_signal.set("The dropped file could not be opened.".to_string());
                    return;
                };

                read_file_into_state(
                    file,
                    document_signal,
                    status_signal,
                    file_name_signal,
                );
            }));

        let _ = drop_zone
            .add_event_listener_with_callback("drop", drop_handler.as_ref().unchecked_ref());
        drop_handler.forget();
    }

    {
        let document_signal = document_signal;
        let mut status_signal = status_signal;
        let file_name_signal = file_name_signal;

        let change_handler =
            Closure::<dyn FnMut(Event)>::wrap(Box::new(move |event: Event| {
                let Some(target) = event.target() else {
                    status_signal.set("No file input target was available.".to_string());
                    return;
                };

                let Ok(input) = target.dyn_into::<HtmlInputElement>() else {
                    status_signal.set("The file picker target was invalid.".to_string());
                    return;
                };

                let Some(files) = input.files() else {
                    status_signal.set("No file was selected.".to_string());
                    return;
                };

                if files.length() == 0 {
                    status_signal.set("No file was selected.".to_string());
                    return;
                }

                let Some(file) = files.get(0) else {
                    status_signal.set("The selected file could not be opened.".to_string());
                    return;
                };

                read_file_into_state(
                    file,
                    document_signal,
                    status_signal,
                    file_name_signal,
                );
                input.set_value("");
            }));

        let _ = file_input
            .add_event_listener_with_callback("change", change_handler.as_ref().unchecked_ref());
        change_handler.forget();
    }
}

#[cfg(target_arch = "wasm32")]
fn read_file_into_state(
    file: File,
    mut document_signal: Signal<Option<AdifDocument>>,
    mut status_signal: Signal<String>,
    mut file_name_signal: Signal<String>,
) {
    let Ok(reader) = FileReader::new() else {
        status_signal.set("Could not create a file reader.".to_string());
        return;
    };

    let file_name = file.name();
    let file_name_for_load = file_name.clone();
    let reader_for_load = reader.clone();

    let load_handler = Closure::<dyn FnMut(Event)>::wrap(Box::new(move |_event: Event| {
        let result = match reader_for_load.result() {
            Ok(result) => result,
            Err(_) => {
                status_signal.set("The file reader finished without returning data.".to_string());
                return;
            }
        };

        let Some(contents) = result.as_string() else {
            status_signal.set("Only text-based ADIF files are supported right now.".to_string());
            return;
        };

        match parse_adif(&contents) {
            Ok(parsed) => {
                let record_count = parsed.records.len();
                document_signal.set(Some(parsed));
                file_name_signal.set(file_name_for_load.clone());
                status_signal.set(format!(
                    "Loaded `{}` with {} record(s).",
                    file_name_for_load,
                    record_count
                ));
            }
            Err(error) => {
                document_signal.set(None);
                status_signal.set(format!("Could not parse `{}`: {}", file_name_for_load, error));
            }
        }
    }));

    reader.set_onload(Some(load_handler.as_ref().unchecked_ref()));
    load_handler.forget();

    if reader.read_as_text(&file).is_err() {
        status_signal.set(format!("Failed to read `{}` as text.", file_name));
    }
}

#[cfg(target_arch = "wasm32")]
fn open_file_picker() {
    let Some(window) = web_sys::window() else {
        return;
    };

    let Some(document) = window.document() else {
        return;
    };

    let Some(input) = document.get_element_by_id(FILE_INPUT_ID) else {
        return;
    };

    let Ok(input) = input.dyn_into::<HtmlInputElement>() else {
        return;
    };

    let _ = input.click();
}

#[cfg(not(target_arch = "wasm32"))]
fn open_file_picker() {}

#[cfg(target_arch = "wasm32")]
fn download_adif(document: &AdifDocument, file_name: &str) {
    let serialized = serialize_adif(document);

    let array = js_sys::Array::new();
    array.push(&JsValue::from_str(&serialized));

    let properties = BlobPropertyBag::new();
    properties.set_type("text/plain;charset=utf-8");

    let Ok(blob) = Blob::new_with_str_sequence_and_options(&array, &properties) else {
        return;
    };

    let Ok(object_url) = Url::create_object_url_with_blob(&blob) else {
        return;
    };

    let Some(window) = web_sys::window() else {
        let _ = Url::revoke_object_url(&object_url);
        return;
    };

    let Some(document) = window.document() else {
        let _ = Url::revoke_object_url(&object_url);
        return;
    };

    let Ok(anchor) = document.create_element("a") else {
        let _ = Url::revoke_object_url(&object_url);
        return;
    };

    let Ok(anchor) = anchor.dyn_into::<HtmlAnchorElement>() else {
        let _ = Url::revoke_object_url(&object_url);
        return;
    };

    anchor.set_href(&object_url);
    anchor.set_download(file_name);

    if let Some(body) = document.body() {
        let _ = body.append_child(&anchor);
        anchor.click();
        let _ = body.remove_child(&anchor);
    }

    let _ = Url::revoke_object_url(&object_url);
}

#[cfg(not(target_arch = "wasm32"))]
fn download_adif(_document: &AdifDocument, _file_name: &str) {}
