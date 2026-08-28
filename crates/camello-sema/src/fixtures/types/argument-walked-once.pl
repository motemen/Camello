package Thing;
use strict;
use warnings;

# A call's arguments are typed once, on the way in, and the parameter check
# reads what that walk produced rather than walking them again. Walking twice
# says whatever the argument has to say twice — and on the right-nested tree a
# Perl list operator builds, it costs 2^depth.

sub new {
    my ($class) = @_;
    return bless {}, $class;
}

sub take {
    my ($self, $value) = @_;
    return $value;
}

sub run {
    my ($self) = @_;
    my $thing = Thing->new;
    return $self->take( $thing->nope );
    #~ warning unknown-method: `Thing` declares no method `nope`
}

1;
