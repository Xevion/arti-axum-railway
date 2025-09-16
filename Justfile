demo:
	docker stop --time=1 arti-demo || true
	docker rm arti-demo || true
	docker build -t arti-demo .
	docker run --detach --publish 8080:8080 --name arti-demo arti-demo
	timeout 10s docker logs -f arti-demo || true
	just demo-address

demo-address:
	docker exec arti-demo ./arti -c /etc/arti/onionservice.toml hss --nickname demo onion-address