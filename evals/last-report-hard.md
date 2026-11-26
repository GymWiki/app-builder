## Per class, variant and model (averages per run)

| class   | variant  | model  | runs | success | clean first time | input | output | cache read | cache write | cost   | time |
| ------- | -------- | ------ | ---- | ------- | ---------------- | ----- | ------ | ---------- | ----------- | ------ | ---- |
| complex | baseline | haiku  | 6    | 100%    | 0%               | 0k    | 7k     | 709k       | 34k         | $0.032 | 64s  |
| complex | baseline | opus   | 6    | 100%    | 67%              | 0k    | 5k     | 413k       | 24k         | $0.516 | 79s  |
| complex | baseline | sonnet | 6    | 100%    | 0%               | 0k    | 4k     | 457k       | 29k         | $0.377 | 64s  |
| complex | router   | haiku  | 6    | 83%     | 50%              | 0k    | 6k     | 558k       | 23k         | $0.018 | 58s  |
| complex | router   | opus   | 6    | 100%    | 100%             | 0k    | 6k     | 474k       | 22k         | $0.380 | 84s  |
| complex | router   | sonnet | 6    | 100%    | 33%              | 0k    | 4k     | 482k       | 20k         | $0.276 | 63s  |
| normal  | baseline | haiku  | 3    | 100%    | 0%               | 0k    | 12k    | 876k       | 38k         | $0.043 | 101s |
| normal  | baseline | opus   | 3    | 100%    | 67%              | 0k    | 7k     | 592k       | 26k         | $0.582 | 112s |
| normal  | baseline | sonnet | 3    | 67%     | 0%               | 0k    | 4k     | 412k       | 23k         | $0.285 | 60s  |
| normal  | router   | haiku  | 3    | 100%    | 100%             | 0k    | 10k    | 655k       | 29k         | $0.017 | 70s  |
| normal  | router   | opus   | 3    | 100%    | 100%             | 0k    | 5k     | 391k       | 19k         | $0.330 | 77s  |
| normal  | router   | sonnet | 3    | 100%    | 100%             | 0k    | 3k     | 312k       | 17k         | $0.134 | 46s  |

## Router and profile versus baseline, per model (same tasks)

| model  | variant  | runs | success | fresh tokens (input+output+cache write) | all tokens | cost  | time |
| ------ | -------- | ---- | ------- | --------------------------------------- | ---------- | ----- | ---- |
| haiku  | router   | 9    | 89%     | 32k                                     | 623k       | $0.02 | 62s  |
| haiku  | baseline | 9    | 100%    | 44k                                     | 809k       | $0.04 | 77s  |
| opus   | router   | 9    | 100%    | 26k                                     | 473k       | $0.36 | 81s  |
| opus   | baseline | 9    | 100%    | 31k                                     | 503k       | $0.54 | 90s  |
| sonnet | router   | 9    | 100%    | 23k                                     | 448k       | $0.23 | 57s  |
| sonnet | baseline | 9    | 89%     | 31k                                     | 473k       | $0.35 | 63s  |

## Per task

| task                   | class   | variant  | haiku          | sonnet         | opus         |
| ---------------------- | ------- | -------- | -------------- | -------------- | ------------ |
| h01-euro-everywhere    | complex | router   | ok $0.03 33k   | ok $0.25 21k   | ok $0.21 17k |
| h01-euro-everywhere    | complex | baseline | ok $0.04 62k   | ok $0.59 59k   | ok $0.26 23k |
| h02-delete-bug         | normal  | router   | ok $0.01 18k   | ok $0.10 15k   | ok $0.16 14k |
| h02-delete-bug         | normal  | baseline | ok $0.03 34k   | ok $0.31 26k   | ok $0.32 27k |
| h03-currency-migration | complex | router   | ok $0.01 25k   | ok $0.32 25k   | ok $0.40 28k |
| h03-currency-migration | complex | baseline | ok $0.03 35k   | ok $0.30 26k   | ok $0.35 28k |
| h04-expense-detail     | complex | router   | FAIL $0.01 24k | ok $0.44 31k   | ok $0.50 35k |
| h04-expense-detail     | complex | baseline | ok $0.05 47k   | ok $0.38 29k   | ok $0.33 27k |
| h05-money-math         | complex | router   | ok $0.03 34k   | ok $0.35 24k   | ok $0.29 21k |
| h05-money-math         | complex | baseline | ok $0.02 33k   | ok $0.35 28k   | ok $0.38 30k |
| h06-empty-state        | normal  | router   | ok $0.02 37k   | ok $0.13 19k   | ok $0.38 25k |
| h06-empty-state        | normal  | baseline | ok $0.04 49k   | ok $0.41 31k   | ok $0.68 22k |
| h07-settle-up          | normal  | router   | ok $0.03 61k   | ok $0.17 27k   | ok $0.45 32k |
| h07-settle-up          | normal  | baseline | ok $0.06 67k   | FAIL $0.14 24k | ok $0.75 53k |
| h08-favorites          | complex | router   | ok $0.02 38k   | ok $0.19 27k   | ok $0.61 43k |
| h08-favorites          | complex | baseline | ok $0.03 37k   | ok $0.40 32k   | ok $1.10 41k |
| h09-stale-total        | complex | router   | ok $0.01 20k   | ok $0.10 16k   | ok $0.27 19k |
| h09-stale-total        | complex | baseline | ok $0.02 28k   | ok $0.24 23k   | ok $0.66 27k |
