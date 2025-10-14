sub bad_number ($1) { }
sub bad_digits ($123) { }
sub bad_negative ($-foo) { }
sub bad_plus_equals ($value += 1) { }
sub bad_low_precedence_or ($x = 1 or 2) { }
sub bad_low_precedence_and ($x = 1 and 2) { }
