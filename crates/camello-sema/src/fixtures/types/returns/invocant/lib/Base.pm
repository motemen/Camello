package Base;
use strict;
use warnings;

sub new {
    my ($class, %args) = @_;
    return bless {%args}, $class;
}

# A builder: what it hands back is the class it was *called* on, not the one
# it was written in (`docs/return-inference.md`, "`$self` comes back as the
# caller's class").
sub set_x {
    my ($self, $x) = @_;
    $self->{x} = $x;
    return $self;
}

# The same, written as a tail.
sub touched {
    my ($self) = @_;
    $self->{touched} = 1;
    $self;
}

# An invocant joined with something else keeps the `Undef`: the call site
# substitutes the `InstanceOf` member and leaves the rest alone.
sub if_ready {
    my ($self) = @_;
    return $self->{ready} ? $self : undef;
}

1;
