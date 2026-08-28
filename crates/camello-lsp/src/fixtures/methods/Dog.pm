package Dog;
use strict;
use warnings;
use parent -norequire, 'Animal';

# Returns: Str
sub speak {
    my ($self) = @_;
    return 'woof';
}

# Returns: Int
sub legs {
    my ($self) = @_;
    return 4;
}

1;
