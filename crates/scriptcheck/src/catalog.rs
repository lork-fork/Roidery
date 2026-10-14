//! The names the checker looks for, each list with the public page it comes from, and the need each
//! feature sets. Only names are taken from these pages; no description or example is copied.

use crate::Need;

// The preprocessor directives, written `#name` or `//@name`:
// https://extendscript.docsforadobe.dev/extendscript-tools-features/preprocessor-directives/
pub(crate) const DIRECTIVES: &[&str] = &["include", "includepath", "script", "strict", "target", "targetengine"];

// The `$` object's environment and file functions:
// https://extendscript.docsforadobe.dev/extendscript-tools-features/dollar-object/
pub(crate) const DOLLAR: &str = "$";
pub(crate) const DOLLAR_ENV: &[&str] = &["getenv", "setenv"];
pub(crate) const DOLLAR_EVAL_FILE: &str = "evalFile";

// https://extendscript.docsforadobe.dev/extendscript-tools-features/user-notification-dialogs/
pub(crate) const NOTIFY: &[&str] = &["alert", "confirm", "prompt"];

// https://extendscript.docsforadobe.dev/extendscript-tools-features/localizing-extendscript-strings/
pub(crate) const LOCALIZE: &str = "localize";

// https://extendscript.docsforadobe.dev/extendscript-tools-features/specifying-measurement-values/
pub(crate) const UNIT_VALUE: &str = "UnitValue";

// File and Folder objects, their functions and their open modes:
// https://extendscript.docsforadobe.dev/file-system-access/file-object/
// https://extendscript.docsforadobe.dev/file-system-access/folder-object/
pub(crate) const FILE: &str = "File";
pub(crate) const FOLDER: &str = "Folder";
pub(crate) const FILE_READS: &[&str] = &["read", "readch", "readln"];
pub(crate) const FOLDER_READS: &[&str] = &["getFiles"];
pub(crate) const FILE_WRITES: &[&str] = &["write", "writeln", "remove", "rename", "copy", "createAlias", "create"];
pub(crate) const FILE_DIALOGS: &[&str] = &["openDialog", "saveDialog"];
pub(crate) const FOLDER_DIALOGS: &[&str] = &["selectDialog"];
pub(crate) const INSTANCE_DIALOGS: &[&str] = &["openDlg", "saveDlg", "selectDlg"];
pub(crate) const FOLDER_PLACES: &[&str] =
    &["appData", "appPackage", "commonFiles", "current", "desktop", "myDocuments", "startup", "system", "temp", "trash", "userData"];
pub(crate) const EXECUTE: &str = "execute";
pub(crate) const OPEN: &str = "open";
/// The open modes that write: write, append and edit.
pub(crate) const WRITE_MODES: &[&str] = &["w", "a", "e"];

// Windows and their types, and drawing:
// https://extendscript.docsforadobe.dev/user-interface-tools/window-object/
// https://extendscript.docsforadobe.dev/user-interface-tools/resource-specifications/
// https://extendscript.docsforadobe.dev/user-interface-tools/graphic-customization-objects/
pub(crate) const WINDOW: &str = "Window";
pub(crate) const ON_DRAW: &str = "onDraw";
pub(crate) const DRAWING: &[&str] = &["newPen", "newBrush"];

// https://extendscript.docsforadobe.dev/interapplication-communication/bridgetalk-message-object/
pub(crate) const BRIDGE_TALK: &str = "BridgeTalk";
// https://extendscript.docsforadobe.dev/external-communication/socket-object/
pub(crate) const SOCKET: &str = "Socket";
// https://extendscript.docsforadobe.dev/integrating-external-libraries/externalobject-object/
pub(crate) const EXTERNAL_OBJECT: &str = "ExternalObject";
// https://extendscript.docsforadobe.dev/scripting-xmp/xmpscript-object-reference/
pub(crate) const XMP: &[&str] = &["XMPMeta", "XMPFile", "XMPUtils", "XMPConst", "XMPDateTime"];
// https://extendscript.docsforadobe.dev/integrating-xml/the-xml-object/
pub(crate) const XML: &[&str] = &["XML", "XMLList"];
// The operators a script can define for its own classes:
// https://extendscript.docsforadobe.dev/extendscript-tools-features/operator-overloading/
pub(crate) const OPERATORS: &[&str] = &["+", "-", "*", "/", "%", "^", "<", "<=", "==", "<<", ">>", ">>>", "&", "|", "===", "~"];
// https://extendscript.docsforadobe.dev/extendscript-tools-features/extendscript-reflection-interface/
pub(crate) const REFLECT: &str = "reflect";

// The roots of the document model: https://ai-scripting.docsforadobe.dev/jsobjref/Application/
pub(crate) const DOM_ROOTS: [&str; 4] = ["app", "activeDocument", "documents", "selection"];

// Application methods that page doesn't list, named in public forum posts:
// https://community.adobe.com/t5/illustrator-discussions/changes-in-cs6/td-p/4173362 (executeMenuCommand,
// doScript, loadAction, unloadAction) and
// https://community.adobe.com/t5/illustrator-discussions/what-s-new-in-illustrator-scripting-cc2020/m-p/10773439
// (selectTool).
pub(crate) const MENU_COMMAND: &str = "executeMenuCommand";
pub(crate) const ACTIONS: &[&str] = &["doScript", "loadAction", "unloadAction"];
pub(crate) const SELECT_TOOL: &str = "selectTool";
/// The application methods above, which the report lists as features, not as member chains.
pub(crate) const APP_METHODS: &[&str] = &["executeMenuCommand", "doScript", "loadAction", "unloadAction", "selectTool"];
/// Text an action set written out at run time holds.
pub(crate) const ACTION_TEXT: &str = "/actionCount";

/// Constructors of the language (ECMA-262 3rd edition, section 15) and of E4X (ECMA-357 2nd edition,
/// section 13), and the platform's own: `new` of these is no document-model type.
pub(crate) const CONSTRUCTORS: &[&str] = &[
    "Object",
    "Function",
    "Array",
    "String",
    "Boolean",
    "Number",
    "Date",
    "RegExp",
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "Namespace",
    "QName",
    "XML",
    "XMLList",
    "File",
    "Folder",
    "Window",
    "Socket",
    "BridgeTalk",
    "ExternalObject",
    "UnitValue",
    "XMPMeta",
    "XMPFile",
    "XMPDateTime",
];

/// Every feature id a report can list, in the order the report lists them.
pub const FEATURES: &[&str] = &[
    "actions",
    "bridgeTalk",
    "directive.include",
    "directive.other",
    "directive.target",
    "directive.targetengine",
    "dollar",
    "dollar.env",
    "dollar.evalFile",
    "dynamicCode",
    "e4x",
    "externalObject",
    "file.dialog",
    "file.execute",
    "file.objects",
    "file.places",
    "file.read",
    "file.write",
    "hostCheck",
    "localize",
    "menuCommand",
    "notify",
    "operatorOverloading",
    "preferences",
    "reflection",
    "scriptui.dialog",
    "scriptui.drawing",
    "scriptui.palette",
    "scriptui.window",
    "selectTool",
    "socket",
    "unitValue",
    "xml",
    "xmp",
];

/// The need feature `id` sets.
pub(crate) fn need(id: &str) -> Option<Need> {
    Some(match id {
        "directive.target" | "directive.include" | "directive.other" | "dollar" | "dynamicCode" | "notify" | "localize" | "unitValue"
        | "preferences" | "hostCheck" => Need::Language,
        "dollar.env" | "dollar.evalFile" | "file.objects" | "file.read" | "file.write" | "file.dialog" | "file.places" | "file.execute" => {
            Need::Files
        }
        "menuCommand" => Need::MenuCommands,
        "scriptui.dialog" | "scriptui.window" | "scriptui.drawing" => Need::Dialogs,
        "directive.targetengine"
        | "scriptui.palette"
        | "bridgeTalk"
        | "socket"
        | "externalObject"
        | "xmp"
        | "xml"
        | "e4x"
        | "operatorOverloading"
        | "reflection"
        | "actions"
        | "selectTool" => Need::HostOnly,
        _ => return None,
    })
}
