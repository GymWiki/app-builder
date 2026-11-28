## Per class and model

| class   | model  | runs | success | clean first time | avg cost | avg tokens | avg time |
| ------- | ------ | ---- | ------- | ---------------- | -------- | ---------- | -------- |
| complex | haiku  | 6    | 100%    | 100%             | $0.012   | 477k       | 58s      |
| complex | opus   | 6    | 100%    | 100%             | $0.292   | 393k       | 67s      |
| complex | sonnet | 6    | 100%    | 100%             | $0.130   | 285k       | 45s      |
| normal  | haiku  | 5    | 100%    | 100%             | $0.013   | 631k       | 59s      |
| normal  | opus   | 5    | 100%    | 100%             | $0.352   | 396k       | 81s      |
| normal  | sonnet | 5    | 100%    | 100%             | $0.122   | 272k       | 40s      |
| trivial | haiku  | 2    | 100%    | 100%             | $0.006   | 305k       | 26s      |
| trivial | opus   | 2    | 100%    | 100%             | $0.148   | 188k       | 26s      |
| trivial | sonnet | 2    | 100%    | 100%             | $0.074   | 211k       | 24s      |

## Automatic choice versus fixed models (same tasks)

| strategy         | tasks | success | total cost | total tokens |
| ---------------- | ----- | ------- | ---------- | ------------ |
| always-lightest  | 13    | 100%    | $0.15      | 6625k        |
| always-strongest | 13    | 100%    | $3.81      | 4718k        |
| auto             | 13    | 100%    | $2.38      | 4331k        |

## Per task

| task                | expected | studio said | haiku/sonnet/opus ... (success, cost)               |
| ------------------- | -------- | ----------- | --------------------------------------------------- |
| t01-title           | trivial  | trivial     | haiku ok $0.006 · sonnet ok $0.061 · opus ok $0.126 |
| t02-bold            | trivial  | trivial     | haiku ok $0.007 · sonnet ok $0.087 · opus ok $0.170 |
| t03-tooltip         | trivial  | normal      | haiku ok $0.005 · sonnet ok $0.060 · opus ok $0.129 |
| t04-player-reset    | normal   | normal      | haiku ok $0.008 · sonnet ok $0.094 · opus ok $0.282 |
| t05-total           | normal   | normal      | haiku ok $0.008 · sonnet ok $0.076 · opus ok $0.161 |
| t06-settings-screen | complex  | normal      | haiku ok $0.013 · sonnet ok $0.115 · opus ok $0.267 |
| t07-history         | complex  | complex     | haiku ok $0.019 · sonnet ok $0.188 · opus ok $0.468 |
| t08-rename-score    | complex  | complex     | haiku ok $0.009 · sonnet ok $0.103 · opus ok $0.241 |
| t09-minus-bug       | normal   | complex     | haiku ok $0.008 · sonnet ok $0.108 · opus ok $0.195 |
| t10-dark-mode       | complex  | complex     | haiku ok $0.012 · sonnet ok $0.101 · opus ok $0.311 |
| t11-tournament      | complex  | normal      | haiku ok $0.033 · sonnet ok $0.265 · opus ok $0.922 |
| t12-persist         | complex  | complex     | haiku ok $0.016 · sonnet ok $0.207 · opus ok $0.348 |
| t13-reset-bug       | normal   | complex     | haiku ok $0.007 · sonnet ok $0.072 · opus ok $0.191 |
