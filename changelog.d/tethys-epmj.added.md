- Library: `tethys::Error` gains a `PackageNotFound` variant carrying the
  bare package name, so tools embedding tethys can distinguish a missing
  package from other not-found errors without parsing message strings.
  `tethys coupling --package NAME` misses now surface this variant; CLI
  output and exit codes are unchanged.
