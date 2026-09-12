- New `Tethys::query_error_flow()` lists public functions and methods whose return
  type is structurally fallible, classifying each as `Result`, `Option`, `OneOf`,
  or the C# async `Task<...>` convention.
- Qualified paths are recognized, including `std::io::Result<()>` and
  `System.Threading.Tasks.Task<OneOf<Success, Error>>`.
- Selection reads the persisted return type rather than the signature text, so a
  function whose *parameter* is a `Result` is not itself reported as fallible.
