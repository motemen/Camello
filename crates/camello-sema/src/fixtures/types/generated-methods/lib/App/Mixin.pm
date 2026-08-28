package App::Mixin;
use strict;
use warnings;

use Exporter 'import';

# What this list holds is decided at run time, so a package that imports it
# gains methods this pass cannot name (`docs/types.md`, METHOD-6a). And a
# package that exports its own subs is a mixin: `$self` in one of them is
# whichever class imported it, not this one (METHOD-6b).
our @EXPORT = public_functions();

sub public_functions { return qw(render) }

sub render {
    my ($self) = @_;
    return $self->whatever_the_host_class_has;
}

1;
