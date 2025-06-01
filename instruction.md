# 実装すること

単一のエンドポイントを持つ websocket を用いたアプリケーションを作成します。

# 仕様

- WebSocket のエンドポイントは `/ws` とする
- WebSocket の接続が確立されたら、 GitHub の REST API を使用して、以下の処理を行う
  - 最新push順にリポジトリを3件取得する
  - それらのリポジトリのworkflow runsをそれぞれ2件ずつ取得する
  - 全リポジトリのworkflow runsを取得し終わったら、全てのworkflow runsをまとめて作成日時順でソートし、以下の形式でクライアントに送信する
  ```json
    {
      "runs": [
        {
          "repositoryName": "takumi3488/kubernetes_manifests",
          "id": 123456789,
          "workflowName": "CI",
          "displayTitle": "CI #123",
          "event": "push",
          "status": "completed",
          "createdAt": "2023-10-01T12:34:56Z",
          "updatedAt": "2023-10-01T12:45:00Z",
          "htmlUrl": "https://github.com/takumi3488/kubernetes_manifests/actions/runs/15377740718"
        },
        // ...以下19件のworkflow runs
      ]
    }
  - クライアントに送信した後、再度 リポジトリのworkflow runsをそれぞれ2件ずつ取得する、クライアントに送信するまでの処理を繰り返す
  - workflow runsの取得は、12秒に1回のペースで行う
  - workflow runsの取得が 5 回終わったら、再度最新push順にリポジトリを3件取得するから始める
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
- GraphQL API の取得には `reqwest` を使用する
- WebSocket の実装には `axum::extract::ws` を使用する
- できるだけI/Oを含まないユニットテストを充実させる

# 注意点
- GitHub API のレート制限に注意すること
  - 1時間あたりのリクエスト数は、認証済みで最大5000回
  - 12秒に1回のペースでリクエストを行うため、1時間あたり300件のリポジトリを取得できる
  - 制限を超えた場合は、エラーを返すのではなく、単に次のリクエストまで待機する
