- `tethys index --trust-msbuild --allow-restore` preserves multi-target project
  assets rather than replacing them one framework at a time.
- Authorized restore validates target-supplied package versions and downloads,
  including projects with `RestoreConfigFile`, without treating successful exit
  status or artifact timestamps alone as proof of currentness.
