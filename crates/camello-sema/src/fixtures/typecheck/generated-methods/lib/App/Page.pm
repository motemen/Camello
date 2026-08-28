package App::Page;
use strict;
use warnings;

use App::Accessors;
use App::Named;

App::Accessors->mk_fields(__PACKAGE__, [qw(title body)]);

sub new { my ($class) = @_; return bless {}, $class }

1;
