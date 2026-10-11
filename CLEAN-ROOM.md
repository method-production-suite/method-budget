# Clean-room rules

Method Budget is an independent, clean-room budgeting tool. It is not affiliated
with Entertainment Partners or Movie Magic Budgeting.

**Allowed sources**
- How film/TV budgets actually work: chart-of-accounts conventions, union and guild rate
  books, payroll fringe rules, public industry training material.
- Observed behaviour of other budgeting tools, used normally through their UI.
- Files that those tools *export* in open formats (Excel, CSV, PDF) for import.
- Studio and network charts of accounts, which studios share with their production partners
  so budgets are built to the right chart.

**Never**
- Decompile, disassemble or debug another vendor's binaries.
- Copy their code, help text, icons or bundled data (rate tables, fringe libraries).
- Reverse engineer a proprietary native file format (e.g. `.mbd`). Native interop needs a
  lawyer's sign-off first and is out of scope for now.

Our own native format is open and documented in this repo.

**Native file conversion** lives outside this repository, as an optional separate tool that
converts to and from our documented `.mthd` format. Its code, notes and guides must never be
read by anyone working on Method Budget's code, and must never be copied, linked or vendored
here.
