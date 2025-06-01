# 実装すること

単一のエンドポイントを持つ websocket を用いたアプリケーションを作成します。

# 仕様

- WebSocket のエンドポイントは `/ws` とする
- WebSocket の接続が確立されたら、以下の GraphQL クエリを全てのリポジトリに渡るように実行する
  ```graphql
  query GetMyWorkflowRunsFixed20RunsPerPage(
    $numReposToFetch: Int = 50,     # 1ページあたりに取得するリポジトリ数
    $repoPageCursor: String         # リポジトリリストのページネーションカーソル (初回はnull)
  ) {
    viewer {
      repositories(
        first: $numReposToFetch,
        after: $repoPageCursor,
        ownerAffiliations: OWNER,       # 自身が所有するリポジトリ
        orderBy: {field: NAME, direction: ASC} # リポジトリ名を昇順でソート
      ) {
        totalCount # 自身が所有する総リポジトリ数
        pageInfo { # リポジトリのページネーション情報
          endCursor
          hasNextPage
          startCursor
        }
        edges { # リポジトリのリスト (カーソル付き)
          cursor # このリポジトリのカーソル
          node { # リポジトリの情報
            nameWithOwner # owner/repository 形式の名前
            url           # リポジトリのURL

            workflowRuns(
              first: 5, # ★ 各リポジトリのランを先頭20件に固定
              orderBy: {field: CREATED_AT, direction: DESC} # 作成日時の降順 (新しい順)
            ) {
              totalCount # ★ このリポジトリのランの総数 (20件で打ち切られたか確認用)
              nodes {    # ★ workflowRuns はページネーション不要なので直接 nodes を取得
                name         # ワークフローランの名前
                databaseId   # ワークフローランのグローバルID
                workflow {   # 親ワークフローの情報
                  name       # ワークフロー定義ファイルの名前
                }
                displayTitle # ワークフローランの表示タイトル
                runNumber    # リポジトリ内でのランの連番
                event        # ランをトリガーしたイベント
                status       # ランの現在の状態
                conclusion   # ランの最終結果
                createdAt    # 作成日時 (ISO 8601形式)
                updatedAt    # 最終更新日時 (ISO 8601形式)
                url          # ワークフローランのURL
              }
            }
          }
        }
      }
    }
  }
  ```
  - ただし、後述するように Cynic を使用して GraphQL クエリを実行するため、上記のクエリを `Cynic` のクエリ用の構造体に変換する必要がある
- 各 GraphQL クエリはレート制限に注意して、12秒に1回だけ実行する
- 全リポジトリのワークフローランを取得し終わったら、全てのワークフローランをまとめて作成日時順でソートし、上位20件を以下の形式でクライアントに送信する
  ```json
  {
    "runs": [
      {
        "repositoryName": "takumi3488/kubernetes_manifests",
        "databaseId": 123456789,
        "workflowName": "CI",
        "displayTitle": "CI #123",
        "runNumber": 123,
        "event": "push",
        "status": "completed",
        "conclusion": "success",
        "createdAt": "2023-10-01T12:34:56Z",
        "updatedAt": "2023-10-01T12:45:00Z",
        "url": "https://github.com/takumi3488/kubernetes_manifests/actions/runs/15377740718"
      },
      // ...以下19件のワークフローラン
    ]
  }
- クライアントに送信した後、再度 1 から GraphQL クエリを実行し、クライアントに送信するまでの処理を繰り返す
- WebSocket へ複数のクライアントが接続している場合、全てのクライアントに同じデータを送信する
- クライアントは WebSocket 接続を閉じることができる

# 実装方針

- Webフレームワークとして axum を使用する
- 簡単なヘキサゴナルアーキテクチャで実装する
- 構成は以下の通り（省略しているファイルもあるため、必要に応じてファイルを追加する）
  ```
  src/
  ├── main.rs
  ├── lib.rs
  ├── domain/
  │   ├── models/ // エンティティ、値オブジェクトなど
  │   │   └── run.rs // 必要であれば他にも追加
  │   ├── models.rs
  │   ├── external_apis/
  │   │   └── github.rs // GitHub APIのインターフェース
  │   └── external_apis.rs
  ├── domain.rs
  ├── application/
  │   ├── use_cases/
  │   │   └── stream_github_actions_runs.rs
  │   ├── use_cases.rs
  │   ├── services/ // use_casesが重くなったらここに処理を切り出す
  │   └── services.rs
  ├── application.rs
  └── infrastructures/
      └── adapters/
          ├── primary/ // プライマリアダプタ
          │   ├── web/ // フレームワーク関連
          │   └── web.rs
          └── secondary/ // セカンダリアダプタ
              ├── external_apis/
              │   └── github.rs // GitHub APIの実装
              └── external_apis.rs
  ```
- mod.rs は使わずに、ディレクトリと同名のファイルを使用する
- GraphQL クエリの実行には `Cynic` クレートを使用する
  - 必要であれば、ユーザーに https://generator.cynic-rs.dev/ で `Cynic` のクエリ用の構造体を生成するように要求する
- WebSocket の実装には `axum::extract::ws` を使用する
- できるだけI/Oを含まないユニットテストを充実させる

# 注意点
- GitHub API のレート制限に注意すること
  - 1時間あたりのリクエスト数は、認証済みで最大5000回
  - 12秒に1回のペースでリクエストを行うため、1時間あたり300件のリポジトリを取得できる
  - 制限を超えた場合は、エラーを返すのではなく、単に次のリクエストまで待機する
