package App::Accessors;
use strict;
use warnings;

# A code generator. The names it makes go into whichever package *called* it,
# and this file is the only one that knows how they are made — so a package
# that calls it is one nobody here can enumerate the methods of
# (`docs/types.md`, METHOD-5g).
sub mk_fields {
    my ($class, $target, $names) = @_;
    for my $name (@$names) {
        no strict 'refs';
        *{"${target}::${name}"} = sub { $_[0]->{$name} };
    }
}

1;
