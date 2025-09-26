my sub foo { 42 }
state sub bar($) { $_[0] }
our sub baz :method :Attr(1) { }
