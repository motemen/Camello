# `use Module VERSION LIST` — the version sits between the module and its import
# list. Reading it as the list's first element leaves the list unparsed, and the
# recovery node it lands in is one the formatter has no rules for.
#
# Only modules that really carry a $VERSION appear here: the oracle in
# scripts/perl-check stubs unknown modules, and a stub fails a version check.
use Exporter 5.57 qw( import );
use POSIX 1.0 qw(floor ceil);
use List::Util 1.45;
use Scalar::Util 1.0 qw(blessed reftype);
use 5.010;
use strict;
use constant PI => 3.14159;
no warnings 1.0 qw(uninitialized);
