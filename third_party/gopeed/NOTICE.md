# Gopeed optional download component

Shannon uses the unmodified Gopeed 1.9.3 Windows x64 web-server executable as an optional, separately running process. Gopeed is not included in the default installer. The user approves download from the upstream GitHub release; both archive and executable hashes are pinned in downloads.rs.

- Project and corresponding source: https://github.com/GopeedLab/gopeed/tree/v1.9.3
- Source archive: https://github.com/GopeedLab/gopeed/archive/refs/tags/v1.9.3.zip
- Upstream binary: https://github.com/GopeedLab/gopeed/releases/tag/v1.9.3
- License: GPL-3.0, full text in LICENSE, also copied beside the installed component.

No upstream source has been modified. The integration invokes a local authenticated HTTP interface. If redistributing the binary independently, preserve applicable license and corresponding-source obligations.
