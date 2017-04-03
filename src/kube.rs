// Copyright 2017 Databricks, Inc.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at

// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Dealing with various kubernetes api calls

use chrono::DateTime;
use chrono::offset::utc::UTC;
use serde::Deserialize;
use hyper::{Client,Url};
use hyper::client::request::Request;
use hyper::client::response::Response;
use hyper::header::{Authorization, Bearer};
use hyper::method::Method;
use hyper::net::HttpsConnector;

use serde_json;
use serde_json::Value;
use hyper_rustls::TlsClient;

use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;

use error::KubeError;

// Various things we can return

// objects
#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub namespace: Option<String>,
    #[serde(rename="creationTimestamp")]
    pub creation_timestamp: Option<DateTime<UTC>>,
}

// pods

#[derive(Debug, Deserialize)]
pub struct PodStatus {
    pub phase: String,
}


#[derive(Debug, Deserialize)]
pub struct Pod {
    pub metadata: Metadata,
    pub status: PodStatus,
}

#[derive(Debug, Deserialize)]
pub struct PodList {
    pub items: Vec<Pod>,
}

// Events
#[derive(Debug, Deserialize)]
pub struct Event {
    pub count: u32,
    pub message: String,
    pub reason: String,
    #[serde(rename="lastTimestamp")]
    pub last_timestamp: DateTime<UTC>,
}

#[derive(Debug, Deserialize)]
pub struct EventList {
    pub items: Vec<Event>,
}



// Nodes
#[derive(Debug, Deserialize)]
pub struct NodeCondition {
    #[serde(rename="type")]
    pub typ: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct NodeStatus {
    pub conditions: Vec<NodeCondition>,
}

#[derive(Debug, Deserialize)]
pub struct NodeSpec {
    pub unschedulable: Option<bool>,
}


#[derive(Debug, Deserialize)]
pub struct Node {
    pub metadata: Metadata,
    pub spec: NodeSpec,
    pub status: NodeStatus,
}

#[derive(Debug, Deserialize)]
pub struct NodeList {
    pub items: Vec<Node>,
}


pub struct Kluster {
    pub name: String,
    endpoint: Url,
    token: String,
    cert_path: String,
    client: Client,
}

impl Kluster {

    fn make_tlsclient(cert_path: &str) -> TlsClient {
        let mut tlsclient = TlsClient::new();
        {
            // add the cert to the root store
            let mut cfg = Arc::get_mut(&mut tlsclient.cfg).unwrap();
            let f = File::open(cert_path).unwrap();
            let mut br = BufReader::new(f);
            let added = cfg.root_store.add_pem_file(&mut br).unwrap();
            if added.1 > 0 {
                println!("[WARNING] Couldn't add some certs from {}", cert_path);
            }
        }
        tlsclient
    }

    pub fn new(name: &str, cert_path: &str, server: &str, token: &str) -> Result<Kluster, KubeError> {


        Ok(Kluster {
            name: name.to_owned(),
            endpoint: try!(Url::parse(server)),
            token: token.to_owned(),
            cert_path: cert_path.to_owned(),
            client: Client::with_connector(HttpsConnector::new(Kluster::make_tlsclient(cert_path))),
        })
    }

    fn send_req(&self, path: &str) -> Result<Response, KubeError> {
        let url = try!(self.endpoint.join(path));
        let req = self.client.get(url);
        let req = req.header(Authorization(
            Bearer {
                token: self.token.clone()
            }
        ));
        req.send().map_err(|he| KubeError::from(he))
    }

    pub fn get<T>(&self, path: &str) -> Result<T, KubeError>
        where T: Deserialize {

        let resp = try!(self.send_req(path));
        serde_json::from_reader(resp).map_err(|sje| KubeError::from(sje))
    }

    // pub fn get_text(&self, path: &str) -> Result<String, KubeError> {
    //     let mut resp = try!(self.send_req(path));
    //     let mut buf = String::new();
    //     resp.read_to_string(&mut buf).map(|_| buf).map_err(|ioe| KubeError::from(ioe))
    // }

    pub fn get_read(&self, path: &str, timeout: Option<Duration>) -> Result<Response, KubeError> {
        if timeout.is_some() {
            let url = try!(self.endpoint.join(path));
            let mut req = try!(Request::with_connector(Method::Get,
                                                       url,
                                                       &HttpsConnector::new(
                                                           Kluster::make_tlsclient(self.cert_path.as_str())
                                                       )));
            { // scope for mutable borrow of req
                let mut headers = req.headers_mut();
                headers.set(Authorization(
                    Bearer {
                        token: self.token.clone()
                    }
                ));
            }
            try!(req.set_read_timeout(timeout));
            let next = try!(req.start());
            next.send().map_err(|he| KubeError::from(he))
        } else {
            self.send_req(path)
        }
    }

    pub fn get_value(&self, path: &str) -> Result<Value, KubeError> {
        let resp = try!(self.send_req(path));
        serde_json::from_reader(resp).map_err(|sje| KubeError::from(sje))
    }
}
