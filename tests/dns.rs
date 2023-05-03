//! Use the DNS provider to test a few things regarding user traces

use std::time::Duration;
use std::process::Command;

use ferrisetw::provider::{Provider, EventFilter};
use ferrisetw::EventRecord;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::UserTrace;
use ferrisetw::trace::TraceTrait;
use ferrisetw::parser::Parser;
use ferrisetw::schema::Schema;

mod utils;
use utils::{Status, TestKind};

const TEST_DOMAIN_NAME: &str = "www.github.com";

const EVENT_ID_DNS_QUERY_INITIATED: u16 = 3006;
const EVENT_ID_DNS_QUERY_COMPLETED: u16 = 3008;



#[test]
fn dns_tests() {
    // These tests must be consecutive, as they share the same DNS provider
    simple_user_dns_trace();
    test_event_id_filter();
}

fn simple_user_dns_trace() {
    let passed = Status::new(TestKind::ExpectSuccess);
    let notifier = passed.notifier();

    let dns_provider = Provider
        ::by_guid("1c95126e-7eea-49a9-a3fe-a378b03ddb4d") // Microsoft-Windows-DNS-Client
        .add_callback(move |record: &EventRecord, schema_locator: &SchemaLocator| {
            let schema = schema_locator.event_schema(record).unwrap();
            let parser = Parser::create(record, &schema);

            // While we're at it, let's check a few more-or-less unrelated things on an actual ETW event
            check_a_few_cases(record, &parser, &schema);

            if has_seen_resolution_to_test_domain(record, &parser) {
                notifier.notify_success();
            }
        })
        .build();

    let dns_trace = UserTrace::new()
        .enable(dns_provider)
        .start_and_process()
        .unwrap();

    generate_dns_events();

    passed.assert_passed();
    assert!(dns_trace.events_handled() > 0);
    dns_trace.stop().unwrap();
    println!("simple_user_dns_trace passed");
}

fn test_event_id_filter() {
    let passed1 = Status::new(TestKind::ExpectSuccess);
    let passed2 = Status::new(TestKind::ExpectNoFailure);
    let passed3 = Status::new(TestKind::ExpectSuccess);

    let notifier1 = passed1.notifier();
    let notifier2 = passed2.notifier();
    let notifier3 = passed3.notifier();

    let filter = EventFilter::ByEventIds(vec![EVENT_ID_DNS_QUERY_COMPLETED]);

    let dns_provider = Provider
        ::by_guid("1c95126e-7eea-49a9-a3fe-a378b03ddb4d") // Microsoft-Windows-DNS-Client
        .add_filter(filter)
        .add_callback(move |record: &EventRecord, _schema_locator: &SchemaLocator| {
            // We want at least one event, but only for the filtered kind
            if record.event_id() == EVENT_ID_DNS_QUERY_COMPLETED {
                notifier1.notify_success();
            } else {
                notifier2.notify_failure();
            }
        })
        .add_callback(move |record: &EventRecord, _schema_locator: &SchemaLocator| {
            // This secondary callback basically tests all callbacks are run
            if record.event_id() == EVENT_ID_DNS_QUERY_COMPLETED {
                notifier3.notify_success();
            }
        })
        .build();

    let _trace = UserTrace::new()
        .enable(dns_provider)
        .start_and_process()
        .unwrap();

    generate_dns_events();

    passed1.assert_passed();
    passed2.assert_passed();
    passed3.assert_passed();
    // Not calling .stop() here, let's just rely on the `impl Drop`

    println!("test_event_id_filter passed");
}


fn generate_dns_events() {
    std::thread::sleep(Duration::from_secs(1));
    // Unfortunately, `&str::to_socket_addrs()` does not use Microsoft APIs, and hence does not trigger a DNS ETW event
    // Let's use ping.exe instead
    println!("Resolving {}...", TEST_DOMAIN_NAME);
    let _output = Command::new("ping.exe")
       .arg("-n")
       .arg("1")
       .arg(TEST_DOMAIN_NAME)
       .output()
       .unwrap();
    println!("Resolution done.");
}

fn check_a_few_cases(record: &EventRecord, parser: &Parser, schema: &Schema) {
    // Parsing with a wrong type should properly error out
    if record.event_id() == EVENT_ID_DNS_QUERY_INITIATED {
        let _right_type: String = parser.try_parse("QueryName").unwrap();
        let wrong_type = parser.try_parse::<u32>("QueryName");
        assert!(wrong_type.is_err());
    }

    // Giving an unknown property should properly error out
    let wrong_name = parser.try_parse::<u32>("NoSuchProperty");
    assert!(wrong_name.is_err());

    assert_eq!(&schema.provider_name(), "Microsoft-Windows-DNS-Client");
}

fn has_seen_resolution_to_test_domain(record: &EventRecord, parser: &Parser) -> bool {
    if record.event_id() == EVENT_ID_DNS_QUERY_INITIATED {
        let query_name: String = parser.try_parse("QueryName").unwrap();
        #[allow(unused_parens)]
        return (query_name == TEST_DOMAIN_NAME);
    }
    false
}
