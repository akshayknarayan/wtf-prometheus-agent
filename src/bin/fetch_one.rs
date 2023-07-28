use color_eyre::eyre::Report;
use wtf_prometheus_agent::{Bound, ElementHealth, Filter};

fn main() -> Result<(), Report> {
    color_eyre::install()?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let mut el = ElementHealth::new(
            "http://localhost:9419/metrics",
            [Filter::Exact {
                metric_name: "rabbitmq_global_messages_unroutable_dropped_total".to_string(),
                trigger: Bound::AbsUpper(1.),
            }],
        )?;
        let triggered_samples = el.check().await?;
        dbg!(triggered_samples);
        Ok::<_, Report>(())
    })?;
    Ok(())
}
