use strict;
use warnings;

my $read = 1;
print $read;

my $ignored = 2;                #~ info unused-variable: `$ignored`

# A leading underscore says "bound on purpose, not read".
my $_deliberate = 3;

sub method {
    # `$self` is unpacked because the shape of @_ says to.
    my ($self, $value) = @_;
    return $value;
}

sub uses_its_parameter ($count) {
    return $count;
}

# A parameter goes on saying what the sub takes whether or not the body reads
# it, so an unread one is its own code rather than `unused-variable`.
sub ignores_its_parameter ($count) {
                                #~ info unused-parameter: `$count`
    return 1;
}

sub ignores_an_unpacked_one {
    my ($self, $wanted, $spare) = @_;
                                #~ info unused-parameter: `$spare`
    return $wanted;
}

sub ignores_a_shifted_one {
    my $self = shift;
    my $spare = shift;          #~ info unused-parameter: `$spare`
    return 1;
}

# `catch ($e)` is bound by the construct whether the body wants it or not, so
# it is neither a variable nobody wanted nor a parameter.
use feature 'try';
sub guarded {
    try {
        return 1;
    }
    catch ($e) {
        return 0;
    }
}

# A value held for its destructor is bound on purpose and never read on
# purpose, so neither code fires on it.
sub takes_a_lock {
    my $guard = Scope::Guard->new(sub { print "released" });
    my $other = Guard::guard { print "released" };
    return 1;
}
