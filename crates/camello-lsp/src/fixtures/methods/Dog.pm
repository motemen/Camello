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

# No `Returns:`, so hover says what the body says — and says that it read it
# rather than that anyone wrote it down (`docs/return-inference.md`).
sub sound {
    my ($self) = @_;
    return 'loud';
}

# A builder hands back the class it was *called* on, and hover shows the
# fallback for a bareword call: the class the sub was written in.
sub rename {
    my ($self, $name) = @_;
    $self->{name} = $name;
    return $self;
}

1;
