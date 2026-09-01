package Widget;
use strict;
use warnings;

sub new {
    my ($class) = @_;
    return bless {}, $class;
}

# Returns: Str
sub name {
    my ($self) = @_;
    return 'widget';
}

# Returns: Int
sub size {
    my ($self) = @_;
    return 1;
}

1;
