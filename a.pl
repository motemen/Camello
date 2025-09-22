        like \@input, [
            map({
                    object {
                        call [ attr => 'value' ] => $_;
                    };
            } @$twitter_hashtags),
            object {
                call [ attr => 'value' ] => '';
            },
        ];

