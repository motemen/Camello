package My::Vars;
use strict;
use warnings;
use Exporter 'import';
our @EXPORT      = qw($Default *Glob);
our @EXPORT_OK   = qw(%Offered);
our %EXPORT_TAGS = (all => [ @EXPORT_OK, qw($Tagged) ]);
our ($Default, @Glob, %Offered, $Tagged);
1;
