# Example Workflows

`flowd` is most useful when you repeat the same local file workflow often enough that rename and move steps become routine.

These examples stay within the current open-core scope:

- observation is local
- suggestions are shown in the CLI
- approved automations are deterministic
- v1 automations focus on file rename and file move actions

The examples below are not promises of broad desktop automation. They show the kinds of repeated file workflows `flowd` can realistically detect and turn into safe suggestions.

Canonical replay fixtures for the strongest examples live in [`fixtures/demo_scenarios/manifest.json`](/Users/nickvandort/Documents/Coding/flowd/fixtures/demo_scenarios/manifest.json). They are intended to stay aligned with these examples for docs, demos, and regression tests.

## 1. Organizing invoices from Downloads

**Repeated user behavior**  
You download invoice PDFs into `~/Downloads`, rename them to match your bookkeeping convention, and move them into `~/Documents/Accounting/Invoices`.

**What flowd detects**  
Repeated sessions where a new PDF appears in `~/Downloads`, is renamed, and is then moved into the invoices folder.

**What automation could be suggested**  
Rename matching invoice PDFs using the learned pattern, then move them into `~/Documents/Accounting/Invoices`.

## 2. Cleaning up screenshots from the Desktop

**Repeated user behavior**  
You take screenshots that land on `~/Desktop`, rename the useful ones, and move them into `~/Pictures/Screenshots`.

**What flowd detects**  
The same screenshot files are repeatedly renamed and moved out of `~/Desktop` into a dedicated archive folder.

**What automation could be suggested**  
Move matching screenshots into `~/Pictures/Screenshots`, optionally preserving the rename pattern you already use.

## 3. Sorting Downloads by document type

**Repeated user behavior**  
You regularly sort downloaded files by renaming them and moving them from `~/Downloads` into folders such as `~/Documents/Manuals` or `~/Documents/Statements`.

**What flowd detects**  
Repeated rename and move sequences for files with similar names, extensions, or destination folders.

**What automation could be suggested**  
Move matching files from `~/Downloads` into the destination folder you consistently use, with the same rename step when applicable.

## 4. Archiving signed PDFs

**Repeated user behavior**  
You receive signed PDFs in a review folder, rename them to mark the final version, and move them into a long-term archive.

**What flowd detects**  
A stable pattern where PDFs are renamed with a suffix such as `-signed` or `-final` and then moved into an archive directory.

**What automation could be suggested**  
Rename matching PDFs using the established suffix or template, then move them into the archive folder.

## 5. Filing monthly bank statements

**Repeated user behavior**  
Each month you download a bank statement PDF, rename it to include the account and month, and move it into `~/Documents/Finance/Statements`.

**What flowd detects**  
The same statement workflow recurring across multiple sessions with similar filenames and the same destination.

**What automation could be suggested**  
Rename matching statement PDFs into the format you already use and move them into the statements folder.

## 6. Normalizing exported reports

**Repeated user behavior**  
You export reports from another local tool, then rename files like `report.pdf` or `export.csv` and move them into a project archive.

**What flowd detects**  
Repeated rename and move actions applied to the same kinds of exported files after they are created.

**What automation could be suggested**  
Rename those exports to the usual project naming pattern and move them into the archive directory.

## 7. Terminal-driven inbox cleanup

**Repeated user behavior**  
You use terminal commands such as `mv` to rename and relocate files from a staging folder into project folders.

**What flowd detects**  
Repeated terminal-observed rename and move flows that map to the same underlying file operations.

**What automation could be suggested**  
A file automation that applies the same rename and move sequence without requiring you to repeat the manual terminal cleanup.

## 8. Repeated project asset renames

**Repeated user behavior**  
You receive files with inconsistent names like `final.png` or `scan.pdf`, rename them to match a project convention, and move them into the correct asset folder.

**What flowd detects**  
The same normalization pattern across multiple files: rename first, then move into the same project directory.

**What automation could be suggested**  
Rename matching assets to your usual convention and move them into the expected project folder.

## 9. Moving reviewed documents out of a hot folder

**Repeated user behavior**  
You review files in an inbox folder, rename the completed ones to indicate status, and move them into `processed` or `archive`.

**What flowd detects**  
Repeated transitions from inbox to processed folders, especially when the rename indicates completion.

**What automation could be suggested**  
Rename matching reviewed files with the completion suffix you already use and move them into the processed destination.

## 10. Preparing receipts for accounting

**Repeated user behavior**  
You collect receipt PDFs or images in a temporary folder, rename them with a date or vendor label, and move them into `~/Documents/Accounting/Receipts`.

**What flowd detects**  
A recurring file organization flow with the same source folder, similar file types, and a stable accounting destination.

**What automation could be suggested**  
Rename matching receipts according to the learned pattern and move them into the receipts archive.

## Guidelines for contributors

Good workflow examples in `flowd` docs should:

- describe repeated local behavior, not one-off tasks
- stay close to file creation, rename, and move patterns
- avoid claims about arbitrary shell execution or GUI automation
- make approval and inspectability clear
- read like workflows users already do by hand
