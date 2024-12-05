use egui::{CollapsingHeader, Ui};

pub struct ReferenceWindow {}

impl ReferenceWindow {
    pub(crate) fn ui(&mut self, ui: &mut Ui) {
        ui.set_min_width(250.0);

        // Title
        ui.heading("Formula Help");

        // Features section
        CollapsingHeader::new("Features")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("The formula engine support:");
                ui.label("• Any numbers, negative and positive, as float or integer.");
                ui.label("• Arithmetic operations: +, -, /, *, ^");
                ui.label("• Logical operations: AND(), OR(), NOT(), XOR().");
                ui.label("• Comparison operations: =, >, >=, <, <=, <>.");
                ui.label("• String operation: & (concatenation).");
                ui.label("• Built-in variables: TRUE, FALSE.");
                ui.label("• Excel functions: ABS(), SUM(), PRODUCT(), AVERAGE(), RIGHT(), LEFT(), IF(), ISBLANK().");
                ui.label("• Operations on lists of values (one-dimensional range).");
                ui.label("• Add or subtract dates and Excel function DAYS().");
                ui.label("• Custom functions with number arguments.");
            });

        CollapsingHeader::new("Examples")
            .default_open(false)
            .show(ui, |ui| {
                self.add_examples(ui);
            });

        // Logical Expressions section
        CollapsingHeader::new("Logical Expressions")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Supports logical expressions like AND(), OR(), and more:");
                ui.monospace("=2>=1");
                ui.monospace("=OR(1>1,1<>1)");
                ui.monospace("=AND(\"test\",\"True\", 1, true)");
            });

        // Date Handling section
        CollapsingHeader::new("Handling Dates")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Supports adding, subtracting, and calculating days between dates:");
                ui.label("• Dates must be written in the RFC 3339: e.g., 2019-03-01T02:00:00.000Z");
                ui.monospace("=DAYS(A12, A32)");
            });

        CollapsingHeader::new("References")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Supports referencing other cells:");
                ui.monospace("=A12");
                ui.label("• The demo limits the number of allowed references per cell to 1000.");
            });
    }

    fn add_examples(&self, ui: &mut Ui) {
        CollapsingHeader::new("Parsing and Evaluating Formulas")
            .default_open(true)
            .show(ui, |ui| {
                ui.monospace("=1+2");
                ui.monospace("=(1*(2+3))*2");
                ui.monospace("=1+3/0");
            });

        CollapsingHeader::new("Concatenating Strings")
            .default_open(true)
            .show(ui, |ui| {
                ui.monospace(r#"="Hello " & " World!""#);
                ui.label("• Concatenating number and string results in a #CAST! error.");
            });

        CollapsingHeader::new("Excel Functions")
            .default_open(true)
            .show(ui, |ui| {
                ui.monospace("=ABS(-1)");
                ui.monospace(r#"=SUM(1,2,"3")"#);
                ui.monospace("=PRODUCT(ABS(1),2*1, 3,4*1)");
                ui.monospace("=RIGHT(\"apple\", 3)");
                ui.monospace("=LEFT(\"apple\", 3)");
                ui.monospace("=LEFT(\"apple\")");
                ui.monospace("=IF(TRUE,1,0)");
            });

        CollapsingHeader::new("Working with Lists")
            .default_open(true)
            .show(ui, |ui| {
                ui.monospace("={1,2,3}+{1,2,3}");
            });
    }
}
