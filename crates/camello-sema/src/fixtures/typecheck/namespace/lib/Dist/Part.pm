package Dist::Part;
use strict;
use warnings;

use Dist;

sub new { my ($class) = @_; return bless {}, $class }

sub describe {
    my ($self) = @_;
    # Declared nowhere this run can see, and in the shared library `Dist`
    # loaded. A package below a dynamic one is dynamic too, so nothing is said.
    return $self->_get_native_thing;
}

1;
