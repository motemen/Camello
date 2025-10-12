# Prototypes with semicolons should be recognized as prototypes, not signatures
# Semicolons separate mandatory and optional parameters in prototypes
sub foo($;) { }
sub bar($;$) { }
sub baz($;@) { }
sub qux($$;@) { }
sub quux($;$$) { }
sub corge(@;$) { }
