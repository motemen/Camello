package Base;
use strict;
use warnings;
use Class::Accessor::Lite (new => 1, ro => ['name']);

# `$class` is `ClassName['Base']` — this package or something below it
# (`docs/types.md`, INFER-9a). So the call resolves, and what it hands back is
# a `Self`: `Child->build` is a `Child` (INFER-9b).
sub build {
    my $class = shift;
    return $class->new;
}

# The marker crosses one lexical, which is the shape every hand-written
# constructor has.
sub build_slowly {
    my $class = shift;
    my $self  = $class->new;
    $self->name;
    return $self;
}

# A tail is a site too.
sub built {
    my $class = shift;
    $class->new;
}

# Not a `Self`: the class is written out, so the answer is `Base` however the
# sub was called.
sub literal {
    return Base->new;
}

# Not a `Self` either: what comes back is not of the receiver's class.
sub config {
    my $class = shift;
    return Config->new;
}

# Another assignment takes the marker away again.
sub replaced {
    my $class = shift;
    my $self  = $class->new;
    $self = Base->new;
    return $self;
}

# A method the package declares is one every subclass has too, so this
# resolves — and one it does not declare is reported, which is the half of
# INFER-9a that a template method would argue with.
sub uses_helpers {
    my $class = shift;
    $class->helper;
    $class->absent;             #~ warning unknown-method: `Base` declares no method `absent`
}

sub helper { 1 }

1;
