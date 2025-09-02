#!/usr/bin/perl

# Simple s/// test
$str = "hello world";
$str =~ s/world/universe/;

# s/// with different delimiters  
$text =~ s{old}{new}g;
$text =~ s[pattern][replacement]i;
$text =~ s|find|replace|;

# s/// with flags
$data =~ s/foo/bar/gi;