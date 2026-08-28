package Child;
use strict;
use warnings;
use parent -norequire, 'Base';

sub extra {
    my ($self) = @_;
    return $self->{x};
}

1;
