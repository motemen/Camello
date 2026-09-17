package Factory;
use strict;
use warnings;
use Neighbour;

# Neither a `bless` nor a `SUPER::`, so what this hands back is some other
# class's (`docs/types.md`, INFER-2g) — the shape `URI::new` has.
sub new {
    my ($class, $kind) = @_;
    my $implementation = "Factory::" . $kind;
    return $implementation->_build;
}

sub greet { 1 }

1;
