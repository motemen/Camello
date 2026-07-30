# Hash keys holding East Asian Wide characters. The alignment counts
# characters, not display columns: 'あいう' is 5 characters and 8 columns
# wide, 'あいうえおかきくけ' is 11 and 20. Aligned on the character count, the
# output does not line up on screen.
my %map = (
    'あいう'             => 1,
    'あいうえおかきくけ' => 2,
    'plain'              => 3,
);
