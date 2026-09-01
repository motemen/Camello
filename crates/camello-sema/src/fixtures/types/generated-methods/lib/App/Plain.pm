package App::Plain;
use strict;
use warnings;

use App::Named;

sub new { my ($class) = @_; return bless {}, $class }

1;
