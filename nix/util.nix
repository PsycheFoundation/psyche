{
  mergeAttrsetsNoConflicts =
    error: attrsets:
    builtins.foldl' (
      acc: current:
      let
        conflicts = builtins.filter (key: builtins.hasAttr key acc) (builtins.attrNames current);
      in
      if conflicts != [ ] then
        throw "${error} Conflicting keys: ${builtins.toString conflicts}"
      else
        acc // current
    ) { } attrsets;
}
