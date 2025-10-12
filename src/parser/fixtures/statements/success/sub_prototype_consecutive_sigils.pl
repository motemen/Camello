# Prototypes with consecutive sigils should be recognized as prototypes, not signatures
sub foo($$) { }
sub bar($@) { }
sub baz(@$) { }
sub qux($$@) { }
sub quux($$$) { }
sub corge(\@@) { }
sub grault(\@$) { }
