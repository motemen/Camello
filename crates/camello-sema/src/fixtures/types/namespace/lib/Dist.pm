package Dist;
use strict;
use warnings;

# The methods of this distribution are in a shared library. XS registers them
# into the distribution's namespace, and this file is the only place that says
# so — `Dist::Part` below has no idea.
require XSLoader;
XSLoader::load('Dist', '0.01');

1;
