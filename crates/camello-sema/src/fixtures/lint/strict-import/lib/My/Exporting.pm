package My::Exporting;
use strict;
use warnings;
require Exporter;

sub import {
    Exporter->import('ok');
}

1;
