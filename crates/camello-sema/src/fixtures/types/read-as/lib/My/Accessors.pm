package My::Accessors;
use strict;
use warnings;

# A project's own wrapper. Nothing here is a declaration a recogniser could
# read: what it does is hand `Class::Accessor::Typed` the caller's arguments,
# at run time, from a `sub import`. The project says so in `camello.toml`
# instead, and every `use My::Accessors` is then read as the module it wraps.
sub import {
    my ($class, %args) = @_;
    return \%args;
}

1;
