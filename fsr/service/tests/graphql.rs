#![cfg(feature = "graphql")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::FailureKind;
use snapfire_fsr_service::{import_graphql, Field, GraphqlTransport, ImportError, Services, Type, TypeDef, Variant};

const CMS: &str = r#"
type Query {
  post(slug: String!): Post
  allPosts(first: Int, status: Status): [Post!]!
}

type Mutation {
  publish(id: ID!): Post!
}

enum Status {
  DRAFT
  PUBLISHED
}

type Author {
  name: String!
}

type Image {
  url: String!
  width: Int
}

type Post {
  id: ID!
  title: String!
  status: Status!
  author: Author
  coverImage: Image
  score: Float!
}

query PostBySlug($slug: String!) {
  post(slug: $slug) {
    id
    headline: title
    status
    author {
      name
    }
    ...cover
  }
}

fragment cover on Post {
  coverImage {
    url
  }
}

query AllPosts($first: Int, $status: Status) {
  allPosts(first: $first, status: $status) {
    id
    score
  }
}

mutation Publish($id: ID!) {
  publish(id: $id) {
    id
    status
  }
}
"#;

fn record(contract: &snapfire_fsr_service::Contract, name: &str) -> Vec<Field> {
  match contract.types.get(name) {
    Some(TypeDef::Record { fields }) => fields.clone(),
    other => panic!("{name} is {other:?}"),
  }
}

#[test]
fn each_named_operation_is_a_method_its_variables_the_params_and_its_selection_the_answer() {
  let imported = import_graphql(CMS, "cms").unwrap();
  let contract = &imported.contract;
  let service = &contract.services["cms"];
  assert_eq!(service.methods.keys().collect::<Vec<_>>(), ["postBySlug", "allPosts", "publish"]);

  let by_slug = &service.methods["postBySlug"];
  assert_eq!(by_slug.params, vec![Field::new("slug", Type::Str)]);
  assert_eq!(by_slug.returns, Type::named("PostBySlugQuery"));
  assert_eq!(record(contract, "PostBySlugQuery"), vec![Field::new("post", Type::optional(Type::named("PostBySlugQueryPost"))).present(true)]);
  assert_eq!(
    record(contract, "PostBySlugQueryPost"),
    vec![
      Field::new("id", Type::Str).present(true),
      Field::new("headline", Type::Str).present(true),
      Field::new("status", Type::named("Status")).present(true),
      Field::new("author", Type::optional(Type::named("PostBySlugQueryPostAuthor"))).present(true),
      Field::new("coverImage", Type::optional(Type::named("PostBySlugQueryPostCoverImage"))).present(true),
    ]
  );
  assert_eq!(contract.types["Status"], TypeDef::Union { variants: vec![Variant::unit("DRAFT"), Variant::unit("PUBLISHED")] });

  let all = &service.methods["allPosts"];
  assert_eq!(all.params, vec![Field::new("first", Type::optional(Type::I32)), Field::new("status", Type::optional(Type::named("Status")))]);
  assert_eq!(record(contract, "AllPostsQuery"), vec![Field::new("allPosts", Type::list(Type::named("AllPostsQueryAllPosts"))).present(true)]);
  assert_eq!(record(contract, "AllPostsQueryAllPosts"), vec![Field::new("id", Type::Str).present(true), Field::new("score", Type::F64).present(true)]);

  assert_eq!(service.methods["publish"].returns, Type::named("PublishMutation"));
}

#[test]
fn an_operation_goes_out_with_only_the_fragments_it_spreads() {
  let imported = import_graphql(CMS, "cms").unwrap();
  let by_slug = &imported.operations["postBySlug"];
  assert_eq!(by_slug.name, "PostBySlug");
  assert!(by_slug.document.contains("query PostBySlug(") && by_slug.document.contains("fragment cover on Post"), "{}", by_slug.document);
  assert!(!by_slug.document.contains("AllPosts"), "{}", by_slug.document);
  assert!(!imported.operations["allPosts"].document.contains("fragment"), "{}", imported.operations["allPosts"].document);
}

#[test]
fn a_document_the_importer_cannot_type_is_refused_with_where() {
  let refused = |extra: &str| import_graphql(&format!("{CMS}\n{extra}"), "cms").unwrap_err();
  assert!(matches!(refused("type Subscription { postChanged: Post }\nsubscription Live { postChanged { id } }"), ImportError::Unsupported { what, .. } if what.contains("subscription")));
  assert!(refused("{ allPosts { id } }").to_string().contains("anonymous"));
  assert!(matches!(refused("query Broken { post(slug: \"a\") { nope } }"), ImportError::Malformed { what, .. } if what.contains("nope")));
  let abstract_type = "union Hit = Post | Author\nextend type Query { search: [Hit!]! }\nquery Search { search { ... on Post { id } } }";
  assert!(matches!(refused(abstract_type), ImportError::Unsupported { what, .. } if what.contains("Hit")));
}

struct Recorded {
  body: String,
}

fn serve(responses: Vec<&'static str>) -> (String, Arc<Mutex<Vec<Recorded>>>) {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}/graphql", listener.local_addr().unwrap());
  let seen = Arc::new(Mutex::new(Vec::new()));
  let recorder = seen.clone();
  std::thread::spawn(move || {
    for body in responses {
      let Ok((stream, _)) = listener.accept() else { return };
      let mut reader = BufReader::new(stream);
      let mut length = 0usize;
      loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end().to_owned();
        if line.is_empty() {
          break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
          length = value.trim().parse().unwrap_or(0);
        }
      }
      let mut payload = vec![0u8; length];
      reader.read_exact(&mut payload).unwrap();
      recorder.lock().push(Recorded { body: String::from_utf8_lossy(&payload).into_owned() });
      let mut stream = reader.into_inner();
      let response = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
      let _ = stream.write_all(response.as_bytes());
      let _ = stream.flush();
    }
  });
  (base, seen)
}

fn args(pairs: Vec<(&str, Value)>) -> ValueMap {
  pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

#[tokio::test]
async fn a_call_posts_the_operation_and_its_variables_and_answers_the_data() {
  let (endpoint, seen) = serve(vec![r#"{"data":{"post":{"id":"7","headline":"Hello","status":"PUBLISHED","author":null,"coverImage":{"url":"/a.png"}}}}"#]);
  let imported = import_graphql(CMS, "cms").unwrap();
  let services = Services::builder().contract(imported.contract.clone()).default_transport(Arc::new(GraphqlTransport::new(&endpoint, &imported))).build();
  let got = services.bind_anonymous().call("cms", "postBySlug", args(vec![("slug", Value::str("hello"))])).await.unwrap();
  let Value::Map(data) = got else { panic!("{got:?}") };
  let Some(Value::Map(post)) = data.get("post") else { panic!("{data:?}") };
  assert_eq!(post.get("status"), Some(&Value::Variant { tag: "PUBLISHED".to_owned(), payload: None }));
  assert_eq!(post.get("author"), Some(&Value::Null));

  let body: serde_json::Value = serde_json::from_str(&seen.lock()[0].body).unwrap();
  assert_eq!(body["operationName"], "PostBySlug");
  assert_eq!(body["variables"], serde_json::json!({ "slug": "hello" }));
  assert!(body["query"].as_str().unwrap().starts_with("query PostBySlug("), "{body}");
}

#[tokio::test]
async fn an_enum_variable_goes_out_as_its_name() {
  let (endpoint, seen) = serve(vec![r#"{"data":{"allPosts":[{"id":"1","score":2}]}}"#]);
  let imported = import_graphql(CMS, "cms").unwrap();
  let services = Services::builder().contract(imported.contract.clone()).default_transport(Arc::new(GraphqlTransport::new(&endpoint, &imported))).build();
  let status = Value::Variant { tag: "DRAFT".to_owned(), payload: None };
  let got = services.bind_anonymous().call("cms", "allPosts", args(vec![("status", status)])).await.unwrap();
  let Value::Map(data) = got else { panic!("{got:?}") };
  assert_eq!(data.get("allPosts"), Some(&Value::seq(vec![Value::Map(args(vec![("id", Value::str("1")), ("score", Value::F64(2.0))]))])));
  let body: serde_json::Value = serde_json::from_str(&seen.lock()[0].body).unwrap();
  assert_eq!(body["variables"], serde_json::json!({ "status": "DRAFT" }));
}

#[tokio::test]
async fn errors_in_the_answer_fail_the_call_with_the_first_message_and_its_code() {
  let (endpoint, _) = serve(vec![
    r#"{"data":null,"errors":[{"message":"the resolver broke"},{"message":"second"}]}"#,
    r#"{"data":null,"errors":[{"message":"no such post","extensions":{"code":"NOT_FOUND"}}]}"#,
  ]);
  let imported = import_graphql(CMS, "cms").unwrap();
  let services = Services::builder().contract(imported.contract.clone()).default_transport(Arc::new(GraphqlTransport::new(&endpoint, &imported))).build();
  let call = || services.bind_anonymous().call("cms", "postBySlug", args(vec![("slug", Value::str("x"))]));
  let failed = call().await.unwrap_err();
  assert_eq!(failed.kind, FailureKind::Internal);
  assert!(failed.message.contains("the resolver broke"), "{}", failed.message);
  let failed = call().await.unwrap_err();
  assert_eq!(failed.kind, FailureKind::NotFound);
  assert!(failed.message.contains("no such post"), "{}", failed.message);
}
