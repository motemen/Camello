use strict;
use warnings;

my $read = 1;
print $read;

my $ignored = 2;                #~ warning unused-variable: `$ignored`

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

# A parameter is declared by the caller's shape, not by a choice to hold a
# value, so an unread one is not reported.
sub ignores_its_parameter ($count) {
    return 1;
}
