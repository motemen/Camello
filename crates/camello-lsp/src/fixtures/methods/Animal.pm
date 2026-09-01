package Animal;
use strict;
use warnings;

sub new {
    my ($class, %args) = @_;
    return bless { name => $args{name} }, $class;
}

# Returns: Str
sub name {
    my ($self) = @_;
    return $self->{name};
}

# Returns: Str
sub speak {
    my ($self) = @_;
    return 'a noise';
}

1;
